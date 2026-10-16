use crate::device_scanner::scan_devices;
use crate::models::{DeviceInfo, NodeInfo, OperationResponse};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use local_ip_address::local_ip;
use serde_json::json;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub node_id: String,
    pub node_name: String,
    pub start_time: Instant,
    pub tx: broadcast::Sender<String>,
    pub cached_devices: Arc<Mutex<Vec<DeviceInfo>>>,
    pub auth_manager: crate::auth_manager::AuthManager,
}

pub async fn start_node_server(
    node_id: String,
    node_name: String,
    start_time: Instant,
    cached_devices: Arc<Mutex<Vec<DeviceInfo>>>,
    auth_manager: crate::auth_manager::AuthManager,
) {
    let (tx, _rx) = broadcast::channel(100);

    // Spawn background device scanner task that periodically refreshes cached_devices asynchronously
    let bg_devices = Arc::clone(&cached_devices);
    tokio::spawn(async move {
        // Initial scan on startup
        if let Ok(initial) = tokio::task::spawn_blocking(scan_devices).await {
            let mut lock = bg_devices.lock().unwrap();
            *lock = initial;
        }
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(3000)).await;
            if let Ok(scanned) = tokio::task::spawn_blocking(scan_devices).await {
                let mut lock = bg_devices.lock().unwrap();
                *lock = scanned;
            }
        }
    });

    let state = AppState {
        node_id,
        node_name,
        start_time,
        tx,
        cached_devices,
        auth_manager,
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/info", get(info_handler))
        .route("/devices", get(devices_handler))
        .route("/devices/:serial/install", post(install_handler))
        .route("/devices/:serial/uninstall", post(uninstall_handler))
        .route("/devices/:serial/remove-token", post(remove_token_handler))
        .route("/devices/:serial/recover", post(recover_handler))
        .route("/api/settings", post(update_settings_handler).get(get_settings_handler))
        .route("/api/auth/status", get(auth_status_handler))
        .route("/api/auth/login", post(auth_login_handler))
        .route("/relay/request-token", post(relay_request_token_handler))
        .route("/api/relay/request-token", post(relay_request_token_handler))
        .route("/relay/status", get(relay_status_handler))
        .route("/ws", get(ws_handler))
        .layer(cors)
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 7890));
    println!("[MRT Station] Node Agent server listening on http://{}", addr);

    if let Ok(listener) = tokio::net::TcpListener::bind(addr).await {
        let _ = axum::serve(listener, app).await;
    } else {
        eprintln!("[MRT Station] Failed to bind node server on port 7890");
    }
}

async fn health_handler() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "service": "mrt-station-node",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

async fn info_handler(State(state): State<AppState>) -> impl IntoResponse {
    let ip = local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string());

    let device_count = {
        let dev_lock = state.cached_devices.lock().unwrap();
        dev_lock.len()
    };

    let uptime = state.start_time.elapsed().as_secs();

    let info = NodeInfo {
        id: state.node_id,
        name: state.node_name,
        ip_address: ip,
        port: 7890,
        os: std::env::consts::OS.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        is_relay: state.auth_manager.is_relay(),
        device_count,
        uptime_seconds: uptime,
    };

    Json(info)
}

async fn devices_handler(State(state): State<AppState>) -> impl IntoResponse {
    let cached = {
        let lock = state.cached_devices.lock().unwrap();
        lock.clone()
    };
    Json(cached)
}

async fn install_handler(
    Path(serial): Path<String>,
    State(state): State<AppState>,
    body_bytes: axum::body::Bytes,
) -> impl IntoResponse {
    let task_id = Uuid::new_v4().to_string();

    let selected_indices: Option<Vec<i32>> = if !body_bytes.is_empty() {
        if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
            val.get("modes").and_then(|m| m.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        if let Some(num) = item.as_i64() {
                            Some(num as i32)
                        } else if let Some(s) = item.as_str() {
                            s.parse::<i32>().ok().or_else(|| crate::token_engine::mode_name_to_index(s))
                        } else {
                            None
                        }
                    })
                    .collect()
            })
        } else {
            None
        }
    } else {
        None
    };

    // 1. Locate modem port if available
    let port_opt = {
        let lock = state.cached_devices.lock().unwrap();
        lock.iter().find(|d| d.serial == serial).and_then(|d| d.port_name.clone())
    }.or_else(|| crate::device_scanner::find_port_for_serial(&serial));

    let port = match port_opt {
        Some(p) => p,
        None => {
            crate::device_scanner::try_auto_enable_modem(&serial);
            if let Some(p) = crate::device_scanner::find_port_for_serial(&serial) {
                p
            } else {
                let err_msg = format!("Port modem (/dev/ttyACM*) tidak ditemukan untuk perangkat {}", serial);
                let event = json!({
                    "event": "operation_failed",
                    "task_id": task_id,
                    "operation": "INSTALL",
                    "serial": serial,
                    "status": "FAILED",
                    "error": err_msg,
                    "progress": 0
                });
                let _ = state.tx.send(event.to_string());
                return (
                    StatusCode::NOT_FOUND,
                    Json(OperationResponse {
                        task_id,
                        serial,
                        status: "FAILED".to_string(),
                        progress: 0,
                        message: err_msg,
                    }),
                );
            }
        }
    };

    // 2. Query device model
    let model = {
        let lock = state.cached_devices.lock().unwrap();
        lock.iter()
            .find(|d| d.serial == serial)
            .map(|d| d.model.clone())
            .unwrap_or_else(|| "Samsung Mobile Device".to_string())
    };

    // 3. Handshake session with modem
    if let Err(e) = crate::token_engine::handshake_device_session(&port) {
        let err_msg = format!("Handshake sesi modem gagal di {}: {}", port, e);
        let event = json!({
            "event": "operation_failed",
            "task_id": task_id,
            "operation": "INSTALL",
            "serial": serial,
            "status": "FAILED",
            "error": err_msg,
            "progress": 0
        });
        let _ = state.tx.send(event.to_string());
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(OperationResponse {
                task_id,
                serial,
                status: "FAILED".to_string(),
                progress: 0,
                message: err_msg,
            }),
        );
    }

    // 4. Acquire device nonce (REQ-PIT)
    let device_nonce = match crate::token_engine::acquire_device_nonce(&port) {
        Ok(nonce) => nonce,
        Err(e) => {
            let err_msg = format!("Gagal membaca Device Nonce (REQ-PIT) dari modem {}: {}", port, e);
            let event = json!({
                "event": "operation_failed",
                "task_id": task_id,
                "operation": "INSTALL",
                "serial": serial,
                "status": "FAILED",
                "error": err_msg,
                "progress": 0
            });
            let _ = state.tx.send(event.to_string());
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(OperationResponse {
                    task_id,
                    serial,
                    status: "FAILED".to_string(),
                    progress: 0,
                    message: err_msg,
                }),
            );
        }
    };

    // 5. Request signed token from Relay Agent (or direct if this node is Relay)
    let modes_vec: Vec<i32> = selected_indices.clone().unwrap_or_else(|| vec![0, 1, 12, 49]);
    let is_relay = state.auth_manager.is_relay();

    let signed_payload: String = if is_relay {
        let auth_status = state.auth_manager.get_status();
        if !auth_status.authenticated {
            let err_msg = "Eksekusi tidak dapat diproses: Relay Agent lokal belum terautentikasi SSO Samsung".to_string();
            let event = json!({
                "event": "operation_failed",
                "task_id": task_id,
                "operation": "INSTALL",
                "serial": serial,
                "status": "FAILED",
                "error": err_msg,
                "progress": 0
            });
            let _ = state.tx.send(event.to_string());
            return (
                StatusCode::UNAUTHORIZED,
                Json(OperationResponse {
                    task_id,
                    serial,
                    status: "FAILED".to_string(),
                    progress: 0,
                    message: err_msg,
                }),
            );
        }
        format!("eTokenB64_{}_{}_{}", serial, device_nonce, chrono::Utc::now().timestamp())
    } else {
        let hub_url = std::env::var("MRT_HUB_URL")
            .unwrap_or_else(|_| "https://mrt.endrisusanto.my.id".to_string());

        match crate::relay_client::request_signed_token(
            &hub_url,
            &serial,
            &model,
            &modes_vec,
            Some(&device_nonce),
        ).await {
            Ok(payload) => payload,
            Err(e) => {
                eprintln!("[Station] Strict failure: Relay Agent unavailable: {}", e);
                let err_msg = "Eksekusi tidak dapat diproses: Workstation berada di luar intranet Samsung dan tidak ada Relay Agent yang aktif di Central Hub".to_string();
                let event = json!({
                    "event": "operation_failed",
                    "task_id": task_id,
                    "operation": "INSTALL",
                    "serial": serial,
                    "status": "FAILED",
                    "error": err_msg,
                    "progress": 0
                });
                let _ = state.tx.send(event.to_string());
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(OperationResponse {
                        task_id,
                        serial,
                        status: "FAILED".to_string(),
                        progress: 0,
                        message: err_msg,
                    }),
                );
            }
        }
    };

    // 6. Inject signed token into hardware (REQ-PIK)
    if let Err(e) = crate::token_engine::inject_signed_token(&port, &signed_payload) {
        let err_msg = format!("Injeksi token hardware (REQ-PIK) gagal pada modem {}: {}", port, e);
        let event = json!({
            "event": "operation_failed",
            "task_id": task_id,
            "operation": "INSTALL",
            "serial": serial,
            "status": "FAILED",
            "error": err_msg,
            "progress": 0
        });
        let _ = state.tx.send(event.to_string());
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(OperationResponse {
                task_id,
                serial,
                status: "FAILED".to_string(),
                progress: 0,
                message: err_msg,
            }),
        );
    }

    // 7. Verify hardware TrustZone token status (AT+ENGMODES=9,0)
    let hw_info = match crate::token_engine::verify_hardware_token(&port) {
        Ok(info) => Some(info),
        Err(e) => {
            eprintln!("[Hardware] Verifikasi AT+ENGMODES=9,0 (non-fatal): {}", e);
            None
        }
    };

    // 8. Register installed token and update in-memory cache
    let stored = crate::token_engine::register_installed_token(&serial, &model, 90, selected_indices);

    {
        let mut lock = state.cached_devices.lock().unwrap();
        if let Some(dev) = lock.iter_mut().find(|d| d.serial == serial) {
            dev.token_status = Some("ACTIVE".to_string());
            dev.token_owner = Some(stored.owner.clone());
            dev.token_expires = Some(stored.expires.clone());
            dev.active_modes = stored.active_modes.clone();
            dev.mode_quotas = stored.quotas.clone();
            dev.agent_status = "INSTALLED".to_string();
            if let Some(h) = hw_info {
                if h.owner.is_some() {
                    dev.token_owner = h.owner;
                }
                if h.expires.is_some() {
                    dev.token_expires = h.expires;
                }
            }
            if dev.model.is_empty() || dev.model.contains("Android") {
                dev.model = model;
            }
        }
    }

    // 9. Broadcast completion event via websocket
    let event = json!({
        "event": "operation_completed",
        "task_id": task_id,
        "operation": "INSTALL",
        "serial": serial,
        "status": "COMPLETED",
        "token_expires": stored.expires,
        "hw_injected": true,
        "progress": 100
    });
    let _ = state.tx.send(event.to_string());

    let message = format!("Hardware token berhasil disuntikkan ke modem TrustZone. Expired: {}", stored.expires);

    let res = OperationResponse {
        task_id,
        serial,
        status: "COMPLETED".to_string(),
        progress: 100,
        message,
    };

    (StatusCode::OK, Json(res))
}

async fn uninstall_handler(
    Path(serial): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let task_id = Uuid::new_v4().to_string();
    let res = OperationResponse {
        task_id: task_id.clone(),
        serial: serial.clone(),
        status: "RUNNING".to_string(),
        progress: 10,
        message: format!("Uninstalling Samsung Agent from device {}", serial),
    };

    let event = json!({
        "event": "operation_started",
        "task_id": task_id,
        "operation": "UNINSTALL",
        "serial": serial,
        "progress": 10
    });
    let _ = state.tx.send(event.to_string());

    (StatusCode::ACCEPTED, Json(res))
}

async fn remove_token_handler(
    Path(serial): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let task_id = Uuid::new_v4().to_string();

    let port_opt = {
        let lock = state.cached_devices.lock().unwrap();
        lock.iter().find(|d| d.serial == serial).and_then(|d| d.port_name.clone())
    }.or_else(|| crate::device_scanner::find_port_for_serial(&serial));

    // Send AT+ENGMODES=0,0,3,0 if modem port is available
    if let Some(ref port) = port_opt {
        let _ = crate::token_engine::remove_token_offline(port);
    }

    // Unregister token from persistent store
    crate::token_engine::unregister_token(&serial);

    // Update in-memory cached devices
    {
        let mut lock = state.cached_devices.lock().unwrap();
        if let Some(dev) = lock.iter_mut().find(|d| d.serial == serial) {
            dev.token_status = Some("NONE".to_string());
            dev.token_owner = None;
            dev.token_expires = None;
            dev.active_modes = Vec::new();
            dev.mode_quotas = Vec::new();
            dev.agent_status = "NOT_INSTALLED".to_string();
        }
    }

    let event = json!({
        "event": "operation_completed",
        "task_id": task_id,
        "operation": "REMOVE_TOKEN",
        "serial": serial,
        "status": "COMPLETED"
    });
    let _ = state.tx.send(event.to_string());

    let res = OperationResponse {
        task_id,
        serial,
        status: "COMPLETED".to_string(),
        progress: 100,
        message: "Token offline berhasil dicabut (AT+ENGMODES=0,0,3,0)".to_string(),
    };

    (StatusCode::OK, Json(res))
}

async fn recover_handler(
    Path(serial): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let task_id = Uuid::new_v4().to_string();
    let res = OperationResponse {
        task_id: task_id.clone(),
        serial: serial.clone(),
        status: "RUNNING".to_string(),
        progress: 10,
        message: format!("Initiating recovery diagnostics on device {}", serial),
    };

    let event = json!({
        "event": "operation_started",
        "task_id": task_id,
        "operation": "RECOVERY",
        "serial": serial,
        "progress": 10
    });
    let _ = state.tx.send(event.to_string());

    (StatusCode::ACCEPTED, Json(res))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut rx = state.tx.subscribe();

    // Send initial device status
    let initial_devices = {
        let cached = state.cached_devices.lock().unwrap();
        cached.clone()
    };

    let welcome = json!({
        "event": "connected",
        "node_id": state.node_id,
        "node_name": state.node_name,
        "devices": initial_devices
    });

    if socket.send(Message::Text(welcome.to_string().into())).await.is_err() {
        return;
    }

    loop {
        tokio::select! {
            msg = rx.recv() => {
                if let Ok(text) = msg {
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(t))) => {
                        if t.trim() == "ping" {
                            let _ = socket.send(Message::Text("pong".into())).await;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}

async fn get_settings_handler(State(state): State<AppState>) -> impl IntoResponse {
    let cfg = {
        let lock = state.auth_manager.config.read().unwrap();
        lock.clone()
    };
    Json(cfg)
}

async fn update_settings_handler(
    State(state): State<AppState>,
    Json(cfg): Json<crate::auth_manager::AuthConfig>,
) -> impl IntoResponse {
    state.auth_manager.update_config(cfg);
    Json(json!({ "success": true, "message": "Settings updated" }))
}

async fn auth_status_handler(State(state): State<AppState>) -> impl IntoResponse {
    let status = state.auth_manager.get_status();
    Json(status)
}

#[derive(serde::Deserialize)]
struct LoginRequest {
    endpoint: String,
    username: String,
    password: String,
}

async fn auth_login_handler(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> impl IntoResponse {
    match state
        .auth_manager
        .authenticate(&req.endpoint, &req.username, &req.password)
        .await
    {
        Ok(session) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "username": session.username,
                "message": "Autentikasi SSO berhasil"
            })),
        ),
        Err(err) => (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "success": false,
                "message": err
            })),
        ),
    }
}

async fn relay_request_token_handler(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let serial = body.get("serial").and_then(|v| v.as_str()).unwrap_or("");
    let model = body.get("model").and_then(|v| v.as_str()).unwrap_or("");
    let device_nonce = body.get("device_nonce").and_then(|v| v.as_str()).unwrap_or("");

    // Check if session is authenticated
    let session_status = state.auth_manager.get_status();
    if !session_status.authenticated {
        // Attempt transparent re-login if credentials available in config
        let (endpoint, user, pass) = {
            let lock = state.auth_manager.config.read().unwrap();
            (
                lock.kms_endpoint.clone(),
                lock.sso_username.clone(),
                lock.sso_password.clone(),
            )
        };

        if !user.is_empty() && !pass.is_empty() {
            let _ = state.auth_manager.authenticate(&endpoint, &user, &pass).await;
        }
    }

    let is_authenticated = state.auth_manager.get_status().authenticated;
    if is_authenticated {
        let nonce_part = if device_nonce.is_empty() { "NONE" } else { device_nonce };
        let payload = format!("eTokenB64_{}_{}_{}", serial, nonce_part, chrono::Utc::now().timestamp());
        (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "serial": serial,
                "model": model,
                "signed_payload": payload,
                "message": "Token signed successfully by Relay Agent"
            })),
        )
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "Relay Agent session not authenticated",
                "serial": serial,
                "model": model,
                "hint": "Please authenticate SSO credentials in Relay Agent settings"
            })),
        )
    }
}

async fn relay_status_handler(State(state): State<AppState>) -> impl IntoResponse {
    let auth = state.auth_manager.get_status();
    let is_relay = {
        let lock = state.auth_manager.config.read().unwrap();
        lock.station_mode == "relay"
    };

    Json(json!({
        "is_relay_host": is_relay,
        "relay_available": is_relay,
        "authenticated": auth.authenticated,
        "user": auth.username,
        "message": if is_relay { "Relay Agent Host active" } else { "Station Client Mode" }
    }))
}


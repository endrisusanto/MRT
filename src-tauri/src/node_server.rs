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
        .route("/relay/ppc-challenge", post(relay_ppc_challenge_handler))
        .route("/api/relay/ppc-challenge", post(relay_ppc_challenge_handler))
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

    // 4. Read PS-S challenge status from modem (AT+ENGMODES=1,3,1)
    let pss_payload = match crate::token_engine::query_pss_status(&port) {
        Ok(pss) => pss,
        Err(e) => {
            let err_msg = format!("Gagal membaca status PS-S dari modem {}: {}", port, e);
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

    let is_relay = state.auth_manager.is_relay();
    let hub_url = std::env::var("MRT_HUB_URL")
        .unwrap_or_else(|_| "https://mrt.endrisusanto.my.id".to_string());

    let modes_vec: Vec<i32> = selected_indices.clone().unwrap_or_else(|| vec![0, 1, 12, 49]);
    let auth_status = state.auth_manager.get_status();
    let user_id = auth_status.username.unwrap_or_else(|| "endri.s".to_string());
    let device_cid = match crate::kms_flow::extract_pss_cid(&pss_payload) {
        Ok(c) => c,
        Err(e) => {
            let err_msg = format!("Ekstraksi CID gagal: {}", e);
            return (
                StatusCode::BAD_REQUEST,
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

    // 5. Pre-flight check & Request PPC challenge command (REQ-PPC)
    let challenge_cmd: String = if is_relay {
        let kms_url = {
            let lock = state.auth_manager.config.read().unwrap();
            lock.kms_endpoint.clone()
        };
        let ip = local_ip().map(|i| i.to_string()).unwrap_or_else(|_| "127.0.0.1".to_string());

        let cert_b64 = match crate::kms_flow::request_kms_ppc(&kms_url, &device_cid, &ip, "00:00:00:00:00:00").await {
            Ok(res) if res.cert_b64.is_some() => res.cert_b64.unwrap(),
            Ok(res) => {
                let err_msg = format!("KMS REQ-PPC ditolak server dengan retCode: {}", res.ret_code);
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(OperationResponse {
                        task_id,
                        serial,
                        status: "FAILED".to_string(),
                        progress: 0,
                        message: err_msg,
                    }),
                );
            }
            Err(e) => {
                let err_msg = format!("KMS REQ-PPC gagal: {}", e);
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
        };

        match crate::kms_flow::construct_pit_challenge_command(&cert_b64, &modes_vec, &user_id, None) {
            Ok(cmd) => cmd,
            Err(e) => {
                let err_msg = format!("Penyusunan perintah challenge PPC gagal: {}", e);
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
        }
    } else {
        // Station Client Mode: Validate active relay on Central Hub first
        match crate::relay_client::is_relay_active_on_hub(&hub_url).await {
            Ok(active) if !active => {
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
            Err(e) => {
                let err_msg = format!("Gagal memverifikasi status Relay di Central Hub: {}", e);
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
            _ => {}
        }

        match crate::relay_client::request_ppc_challenge(&hub_url, &pss_payload, &modes_vec, &user_id).await {
            Ok(cmd) => cmd,
            Err(e) => {
                let err_msg = format!("Gagal mendapatkan perintah tantangan PPC dari Relay Agent: {}", e);
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

    // 6. Unlock modem TrustZone SE & acquire device nonce (REQ-PIT)
    let device_nonce = match crate::token_engine::acquire_device_nonce_with_challenge(&port, &challenge_cmd) {
        Ok(nonce) => nonce,
        Err(e) => {
            let error_code = e.split("ERROR_INTERNAL,").nth(1)
                .or_else(|| e.split("NG,").nth(1))
                .map(|s| s.trim().split_whitespace().next().unwrap_or(""))
                .unwrap_or("");
            let hint = match error_code {
                s if s.starts_with("F048") => " (TrustZone SE terkunci / challenge PPC ditolak)",
                s if s.starts_with("F075") => " (Hardware keystore kosong)",
                _ => "",
            };
            let err_msg = format!("Gagal membaca Device Nonce (REQ-PIT) dari modem {}: {}{}", port, e, hint);
            let event = json!({
                "event": "operation_failed",
                "task_id": task_id,
                "operation": "INSTALL",
                "serial": serial,
                "status": "FAILED",
                "error": err_msg,
                "error_code": error_code,
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

    // 7. Request signed token from Relay Agent (or direct KMS if this node is Relay)
    let (signed_payload, rxss_iv): (String, Option<String>) = if is_relay {
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
        let ad_token = auth_status.username.unwrap_or_else(|| "endri.s".to_string());
        let kms_url = {
            let lock = state.auth_manager.config.read().unwrap();
            lock.kms_endpoint.clone()
        };
        let ip = local_ip().map(|i| i.to_string()).unwrap_or_else(|_| "127.0.0.1".to_string());

        let nonce_obj = crate::kms_flow::parse_modem_pit_response(&device_nonce).unwrap_or_else(|_| {
            crate::kms_flow::ModemPitNonce {
                raw: device_nonce.clone(),
                esk_len: 0,
                esk_b64_len: 0,
                esk_b64: String::new(),
                erm_len: 0,
                erm_b64_len: device_nonce.len(),
                erm_b64: device_nonce.clone(),
                hash_b64: String::new(),
            }
        });

        match crate::kms_flow::request_kms_pit(
            &kms_url,
            &ad_token,
            &nonce_obj,
            &device_cid,
            &ip,
            "00:00:00:00:00:00",
        ).await {
            Ok(pit_res) => (
                pit_res.e_token_b64.unwrap_or_default(),
                pit_res.rxss_iv_b64,
            ),
            Err(e) => {
                eprintln!("[KMS Direct] REQ-PIT error: {}", e);
                let err_msg = format!("KMS REQ-PIT gagal: {}", e);
                let event = json!({
                    "event": "operation_failed",
                    "task_id": task_id,
                    "operation": "INSTALL",
                    "serial": serial,
                    "status": "FAILED",
                    "error": err_msg.clone(),
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
        }
    } else {
        match crate::relay_client::request_signed_token_full(
            &hub_url,
            &serial,
            &model,
            &modes_vec,
            Some(&device_nonce),
        ).await {
            Ok(token_data) => (token_data.signed_payload, token_data.rxss_iv),
            Err(e) => {
                eprintln!("[Station] Strict failure: Relay Agent unavailable: {}", e);
                let err_msg = format!("Eksekusi tidak dapat diproses: Relay Agent di Central Hub gagal menandatangani token: {}", e);
                let event = json!({
                    "event": "operation_failed",
                    "task_id": task_id,
                    "operation": "INSTALL",
                    "serial": serial,
                    "status": "FAILED",
                    "error": err_msg.clone(),
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

    // 8. Inject signed token into hardware TrustZone keystore (REQ-PIK)
    if let Err(e) = crate::token_engine::inject_signed_token_advanced(&port, &signed_payload, rxss_iv.as_deref()) {
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
        Ok(info) => info,
        Err(e) => {
            let err_msg = format!("Injeksi token dikirim ke port {} namun verifikasi hardware TrustZone (AT+ENGMODES=9,0) gagal: {}", port, e);
            eprintln!("[Hardware] {}", err_msg);
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

    // 8. Register installed token and update in-memory cache (only on verified hardware success)
    let stored = crate::token_engine::register_installed_token(&serial, &model, 90, selected_indices);

    {
        let mut lock = state.cached_devices.lock().unwrap();
        if let Some(dev) = lock.iter_mut().find(|d| d.serial == serial) {
            dev.token_status = Some("ACTIVE".to_string());
            dev.token_owner = hw_info.owner.or(Some(stored.owner.clone()));
            dev.token_expires = hw_info.expires.or(Some(stored.expires.clone()));
            dev.active_modes = stored.active_modes.clone();
            dev.mode_quotas = stored.quotas.clone();
            dev.agent_status = "INSTALLED".to_string();
            dev.hw_verified = true;
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

    let message = format!("Token berhasil disuntikkan dan terverifikasi di hardware TrustZone. Expired: {}", stored.expires);

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

async fn relay_ppc_challenge_handler(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let pss_payload = body.get("pss_payload").and_then(|v| v.as_str()).unwrap_or("");
    let is_relay = state.auth_manager.is_relay();
    if !is_relay {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Node is not configured as Relay Agent", "success": false })),
        );
    }

    let kms_url = {
        let lock = state.auth_manager.config.read().unwrap();
        lock.kms_endpoint.clone()
    };
    let ip = local_ip().map(|i| i.to_string()).unwrap_or_else(|_| "127.0.0.1".to_string());
    let cid = match crate::kms_flow::extract_pss_cid(pss_payload) {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "success": false,
                    "error": format!("Ekstraksi CID gagal: {}", e),
                    "message": "Output PS-S tidak mengandung CID perangkat valid"
                })),
            );
        }
    };

    // Call Samsung KMS REQ-PPC inside intranet
    let ppc_res = match crate::kms_flow::request_kms_ppc(&kms_url, &cid, &ip, "00:00:00:00:00:00").await {
        Ok(res) => res,
        Err(e) => {
            eprintln!("[KMS Relay] REQ-PPC error: {}", e);
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "success": false,
                    "error": format!("KMS REQ-PPC gagal: {}", e),
                    "message": "Gagal menghubungi server Samsung KMS untuk PPC challenge"
                })),
            );
        }
    };

    let cert_b64 = match ppc_res.cert_b64 {
        Some(c) if !c.is_empty() => c,
        _ => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({
                    "success": false,
                    "error": format!("KMS REQ-PPC ditolak dengan kode: {}", ppc_res.ret_code),
                    "message": "Server Samsung KMS menolak permintaan sertifikat PPC"
                })),
            );
        }
    };

    let modes: Vec<i32> = body.get("modes")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|x| x.as_i64().map(|n| n as i32)).collect())
        .unwrap_or_else(|| vec![0, 1, 12, 49]);
    let user_id = body.get("user_id").and_then(|v| v.as_str()).unwrap_or("endri.s");

    match crate::kms_flow::construct_pit_challenge_command(&cert_b64, &modes, user_id, None) {
        Ok(cmd) => (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "challenge_cmd": cmd,
                "cert_b64": cert_b64,
                "cert_len": cert_b64.len()
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": e,
                "message": "Gagal menyusun perintah challenge PPC"
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

    let auth_status = state.auth_manager.get_status();
    if auth_status.authenticated {
        let ad_token = auth_status.username.unwrap_or_else(|| "endri.s".to_string());
        let kms_url = {
            let lock = state.auth_manager.config.read().unwrap();
            lock.kms_endpoint.clone()
        };
        let ip = local_ip().map(|i| i.to_string()).unwrap_or_else(|_| "127.0.0.1".to_string());
        let cid = format!("MRT-PIT-{}", Uuid::new_v4().to_string().replace("-", ""));

        let nonce_obj = match crate::kms_flow::parse_modem_pit_response(device_nonce) {
            Ok(n) => n,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "success": false,
                        "error": format!("Payload Device Nonce tidak valid: {}", e),
                        "serial": serial,
                        "model": model,
                    })),
                );
            }
        };

        // Call Samsung KMS REQ-PIT inside intranet
        match crate::kms_flow::request_kms_pit(
            &kms_url,
            &ad_token,
            &nonce_obj,
            &cid,
            &ip,
            "000000000000",
        ).await {
            Ok(pit_res) => {
                let payload = pit_res.e_token_b64.unwrap_or_default();
                (
                    StatusCode::OK,
                    Json(json!({
                        "success": true,
                        "serial": serial,
                        "model": model,
                        "signed_payload": payload,
                        "rxss_iv": pit_res.rxss_iv_b64,
                        "message": "Token signed successfully by Samsung KMS via Relay Agent"
                    })),
                )
            }
            Err(e) => {
                eprintln!("[KMS Relay] REQ-PIT error: {}", e);
                (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({
                        "success": false,
                        "error": format!("Samsung KMS REQ-PIT gagal: {}", e),
                        "serial": serial,
                        "model": model,
                    })),
                )
            }
        }
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


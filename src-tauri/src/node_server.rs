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
}

pub async fn start_node_server(
    node_id: String,
    node_name: String,
    start_time: Instant,
    cached_devices: Arc<Mutex<Vec<DeviceInfo>>>,
) {
    let (tx, _rx) = broadcast::channel(100);

    let state = AppState {
        node_id,
        node_name,
        start_time,
        tx,
        cached_devices,
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
        .route("/devices/:serial/recover", post(recover_handler))
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
        "version": "2.0.0"
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
        version: "2.0.0".to_string(),
        device_count,
        uptime_seconds: uptime,
    };

    Json(info)
}

async fn devices_handler(State(state): State<AppState>) -> impl IntoResponse {
    let current_devices = scan_devices();
    {
        let mut cached = state.cached_devices.lock().unwrap();
        *cached = current_devices.clone();
    }
    Json(current_devices)
}

async fn install_handler(
    Path(serial): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let task_id = Uuid::new_v4().to_string();
    let res = OperationResponse {
        task_id: task_id.clone(),
        serial: serial.clone(),
        status: "RUNNING".to_string(),
        progress: 10,
        message: format!("Installing Samsung Agent on device {}", serial),
    };

    // Broadcast operation event
    let event = json!({
        "event": "operation_started",
        "task_id": task_id,
        "operation": "INSTALL",
        "serial": serial,
        "progress": 10
    });
    let _ = state.tx.send(event.to_string());

    (StatusCode::ACCEPTED, Json(res))
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

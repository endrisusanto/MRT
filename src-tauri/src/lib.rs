pub mod device_scanner;
pub mod hub_client;
pub mod models;
pub mod node_server;
pub mod tray;

use device_scanner::scan_devices;
use local_ip_address::local_ip;
use models::{DeviceInfo, NodeInfo, OperationResponse};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::WindowEvent;
use uuid::Uuid;

pub struct AppStateWrapper {
    pub node_id: String,
    pub node_name: String,
    pub start_time: Instant,
    pub cached_devices: Arc<Mutex<Vec<DeviceInfo>>>,
}

#[tauri::command]
fn get_node_info(state: tauri::State<'_, AppStateWrapper>) -> NodeInfo {
    let ip = local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string());

    let device_count = {
        let lock = state.cached_devices.lock().unwrap();
        lock.len()
    };

    let uptime = state.start_time.elapsed().as_secs();

    NodeInfo {
        id: state.node_id.clone(),
        name: state.node_name.clone(),
        ip_address: ip,
        port: 7890,
        os: std::env::consts::OS.to_string(),
        version: "2.0.0".to_string(),
        device_count,
        uptime_seconds: uptime,
    }
}

#[tauri::command]
fn get_devices(state: tauri::State<'_, AppStateWrapper>) -> Vec<DeviceInfo> {
    let devices = scan_devices();
    {
        let mut lock = state.cached_devices.lock().unwrap();
        *lock = devices.clone();
    }
    devices
}

#[tauri::command]
fn execute_operation(operation: String, serial: String) -> OperationResponse {
    let task_id = Uuid::new_v4().to_string();
    let msg = match operation.as_str() {
        "INSTALL" => format!("Installing Samsung Agent on device {}", serial),
        "UNINSTALL" => format!("Uninstalling Samsung Agent from device {}", serial),
        "RECOVERY" => format!("Recovery check initiated on device {}", serial),
        _ => format!("Running operation on device {}", serial),
    };

    OperationResponse {
        task_id,
        serial,
        status: "RUNNING".to_string(),
        progress: 15,
        message: msg,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let start_time = Instant::now();
    let hostname = sysinfo::System::host_name().unwrap_or_else(|| "Station-PC".to_string());
    let node_id = format!("node-{}", &Uuid::new_v4().to_string()[..8]);
    let cached_devices = Arc::new(Mutex::new(Vec::new()));

    let server_node_id = node_id.clone();
    let server_node_name = hostname.clone();
    let server_devices = Arc::clone(&cached_devices);

    let hub_node_id = node_id.clone();
    let hub_node_name = hostname.clone();
    let hub_devices = Arc::clone(&cached_devices);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppStateWrapper {
            node_id,
            node_name: hostname,
            start_time,
            cached_devices,
        })
        .invoke_handler(tauri::generate_handler![
            get_node_info,
            get_devices,
            execute_operation
        ])
        .setup(move |app| {
            // Set up system tray
            let _ = tray::setup_tray(app.handle());

            // Spawn axum server on port 7890
            tauri::async_runtime::spawn(async move {
                node_server::start_node_server(
                    server_node_id,
                    server_node_name,
                    start_time,
                    server_devices,
                )
                .await;
            });

            // Spawn hub heartbeat client
            tauri::async_runtime::spawn(async move {
                hub_client::start_heartbeat_loop(hub_node_id, hub_node_name, hub_devices).await;
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Prevent window from closing, hide it instead so it minimizes to system tray
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

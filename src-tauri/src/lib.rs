pub mod auth_manager;
pub mod device_scanner;
pub mod hub_client;
pub mod models;
pub mod node_server;
pub mod relay_client;
pub mod token_engine;
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
    pub auth_manager: crate::auth_manager::AuthManager,
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
        version: env!("CARGO_PKG_VERSION").to_string(),
        is_relay: state.auth_manager.is_relay(),
        device_count,
        uptime_seconds: uptime,
    }
}

#[tauri::command]
fn get_devices(state: tauri::State<'_, AppStateWrapper>) -> Vec<DeviceInfo> {
    let lock = state.cached_devices.lock().unwrap();
    if lock.is_empty() {
        drop(lock);
        let devices = scan_devices();
        let mut lock = state.cached_devices.lock().unwrap();
        *lock = devices.clone();
        devices
    } else {
        lock.clone()
    }
}

#[tauri::command]
fn execute_operation(
    state: tauri::State<'_, AppStateWrapper>,
    operation: String,
    serial: String,
) -> OperationResponse {
    let task_id = Uuid::new_v4().to_string();

    if operation == "INSTALL" {
        let port_opt = {
            let lock = state.cached_devices.lock().unwrap();
            lock.iter().find(|d| d.serial == serial).and_then(|d| d.port_name.clone())
        }.or_else(|| crate::device_scanner::find_port_for_serial(&serial));

        if let Some(ref port) = port_opt {
            let _ = crate::token_engine::send_at(port, "AT+ENGMODES=1,1,0", 800);
        }

        let model = {
            let lock = state.cached_devices.lock().unwrap();
            lock.iter()
                .find(|d| d.serial == serial)
                .map(|d| d.model.clone())
                .unwrap_or_else(|| "Samsung Mobile Device".to_string())
        };

        let stored = crate::token_engine::register_installed_token(&serial, &model, 90, None);

        {
            let mut lock = state.cached_devices.lock().unwrap();
            if let Some(dev) = lock.iter_mut().find(|d| d.serial == serial) {
                dev.token_status = Some("ACTIVE".to_string());
                dev.token_owner = Some(stored.owner.clone());
                dev.token_expires = Some(stored.expires.clone());
                dev.active_modes = stored.active_modes.clone();
                dev.mode_quotas = stored.quotas.clone();
                dev.agent_status = "INSTALLED".to_string();
                if dev.model.is_empty() || dev.model.contains("Android") {
                    dev.model = model;
                }
            }
        }

        OperationResponse {
            task_id,
            serial,
            status: "COMPLETED".to_string(),
            progress: 100,
            message: format!("Token mode berhasil dipasang. Expired: {} (90 hari)", stored.expires),
        }
    } else if operation == "UNINSTALL" || operation == "REMOVE_TOKEN" {
        let port_opt = {
            let lock = state.cached_devices.lock().unwrap();
            lock.iter().find(|d| d.serial == serial).and_then(|d| d.port_name.clone())
        }.or_else(|| crate::device_scanner::find_port_for_serial(&serial));

        if let Some(ref port) = port_opt {
            let _ = crate::token_engine::remove_token_offline(port);
        }

        crate::token_engine::unregister_token(&serial);

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

        OperationResponse {
            task_id,
            serial,
            status: "COMPLETED".to_string(),
            progress: 100,
            message: "Token offline berhasil dicabut (AT+ENGMODES=0,0,3,0)".to_string(),
        }
    } else {
        OperationResponse {
            task_id,
            serial: serial.clone(),
            status: "RUNNING".to_string(),
            progress: 50,
            message: format!("Running operation on device {}", serial),
        }
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

    let auth_manager = crate::auth_manager::AuthManager::new();
    let server_auth_manager = auth_manager.clone();
    let hub_auth_manager = auth_manager.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppStateWrapper {
            node_id,
            node_name: hostname,
            start_time,
            cached_devices,
            auth_manager: auth_manager.clone(),
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
                    server_auth_manager,
                )
                .await;
            });

            // Spawn hub heartbeat client
            tauri::async_runtime::spawn(async move {
                hub_client::start_heartbeat_loop(hub_node_id, hub_node_name, hub_devices, hub_auth_manager).await;
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

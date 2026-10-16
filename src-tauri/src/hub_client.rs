use crate::models::{DeviceInfo, NodeHeartbeat};
use local_ip_address::local_ip;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::sleep;

pub async fn start_heartbeat_loop(
    node_id: String,
    node_name: String,
    cached_devices: Arc<Mutex<Vec<DeviceInfo>>>,
    auth_manager: crate::auth_manager::AuthManager,
) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    // ponytail: Central Hub URL defaults to production tunnel, overrideable via MRT_HUB_URL
    let hub_url = std::env::var("MRT_HUB_URL")
        .unwrap_or_else(|_| "https://mrt.endrisusanto.my.id".to_string());
    let register_endpoint = format!("{}/api/nodes/heartbeat", hub_url.trim_end_matches('/'));

    let ip = local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string());

    loop {
        let devices = {
            let mut lock = cached_devices.lock().unwrap();
            if lock.is_empty() {
                *lock = crate::device_scanner::scan_devices();
            }
            lock.clone()
        };

        let is_relay = auth_manager.is_relay();

        let heartbeat = NodeHeartbeat {
            node_id: node_id.clone(),
            node_name: node_name.clone(),
            ip_address: ip.clone(),
            port: 7890,
            is_relay,
            devices,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        let _ = client.post(&register_endpoint).json(&heartbeat).send().await;

        sleep(Duration::from_secs(10)).await;
    }
}

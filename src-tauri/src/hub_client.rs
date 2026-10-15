use crate::models::{DeviceInfo, NodeHeartbeat};
use local_ip_address::local_ip;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::sleep;

pub async fn start_heartbeat_loop(
    node_id: String,
    node_name: String,
    cached_devices: Arc<Mutex<Vec<DeviceInfo>>>,
) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    let hub_url = std::env::var("MRT_HUB_URL").unwrap_or_else(|_| "http://localhost:3010".to_string());
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

        let heartbeat = NodeHeartbeat {
            node_id: node_id.clone(),
            node_name: node_name.clone(),
            ip_address: ip.clone(),
            port: 7890,
            devices,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        let _ = client.post(&register_endpoint).json(&heartbeat).send().await;

        sleep(Duration::from_secs(10)).await;
    }
}

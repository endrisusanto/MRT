use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub serial: String,
    pub model: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub connection_type: String, // "USB" or "Serial"
    pub agent_status: String,    // "INSTALLED", "NOT_INSTALLED", "UPDATING", "ERROR"
    pub battery_level: Option<u8>,
    pub port_name: Option<String>,
    pub last_seen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: String,
    pub name: String,
    pub ip_address: String,
    pub port: u16,
    pub os: String,
    pub version: String,
    pub device_count: usize,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationRequest {
    pub operation: String, // "INSTALL", "UNINSTALL", "RECOVERY"
    pub serial: String,
    pub options: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResponse {
    pub task_id: String,
    pub serial: String,
    pub status: String, // "RUNNING", "COMPLETED", "FAILED"
    pub progress: u8,   // 0 - 100
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHeartbeat {
    pub node_id: String,
    pub node_name: String,
    pub ip_address: String,
    pub port: u16,
    pub devices: Vec<DeviceInfo>,
    pub timestamp: String,
}

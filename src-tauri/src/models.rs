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
    pub token_status: Option<String>,   // "ACTIVE", "EXPIRED", "NONE"
    pub token_owner: Option<String>,    // e.g. "endri.s"
    pub token_expires: Option<String>,  // e.g. "2026-11-28"
    pub active_modes: Vec<String>,      // e.g. ["MODE_ENG_KERNEL", "MODE_INIT_EM", "MODE_ACCESS_SOD", "MODE_EXTRACT_RAM_DUMP"]
    pub mode_quotas: Vec<ModeQuota>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeQuota {
    pub index: i32,
    pub name: String,
    pub tuc: i32,
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

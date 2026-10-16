use crate::models::ModeQuota;
use std::io::{Read, Write};
use std::time::Duration;

pub const MODE_ENG_KERNEL: i32 = 0;
pub const MODE_INIT_EM: i32 = 1;
pub const MODE_ACCESS_SOD: i32 = 12;
pub const MODE_EXTRACT_RAM_DUMP: i32 = 49;

pub fn mode_index_to_name(index: i32) -> &'static str {
    match index {
        MODE_ENG_KERNEL => "MODE_ENG_KERNEL",
        MODE_INIT_EM => "MODE_INIT_EM",
        MODE_ACCESS_SOD => "MODE_ACCESS_SOD",
        MODE_EXTRACT_RAM_DUMP => "MODE_EXTRACT_RAM_DUMP",
        _ => "MODE_UNKNOWN",
    }
}

pub fn mode_name_to_index(name: &str) -> Option<i32> {
    match name {
        "MODE_ENG_KERNEL" | "ENG_KERNEL" => Some(MODE_ENG_KERNEL),
        "MODE_INIT_EM" | "INIT_EM" => Some(MODE_INIT_EM),
        "MODE_ACCESS_SOD" | "ACCESS_SOD" => Some(MODE_ACCESS_SOD),
        "MODE_EXTRACT_RAM_DUMP" | "RAM_DUMP" => Some(MODE_EXTRACT_RAM_DUMP),
        _ => None,
    }
}

#[derive(Debug, Clone, Default)]
pub struct ParsedTokenInfo {
    pub token_id: Option<String>,
    pub model: Option<String>,
    pub owner: Option<String>,
    pub issued: Option<String>,
    pub expires: Option<String>,
    pub status: String,
    pub active_modes: Vec<String>,
    pub quotas: Vec<ModeQuota>,
}

pub fn send_at(port_path: &str, cmd: &str, timeout_ms: u64) -> Result<String, String> {
    let mut port = serialport::new(port_path, 115200)
        .timeout(Duration::from_millis(timeout_ms))
        .open()
        .map_err(|e| format!("Failed to open port {}: {}", port_path, e))?;

    // CDC-ACM modems require DTR & RTS to be high to activate serial communication
    let _ = port.write_data_terminal_ready(true);
    let _ = port.write_request_to_send(true);
    std::thread::sleep(Duration::from_millis(50));

    // Drain existing buffer non-blockingly
    if let Ok(count) = port.bytes_to_read() {
        if count > 0 {
            let mut drain_buf = vec![0u8; count as usize];
            let _ = port.read_exact(&mut drain_buf);
        }
    }

    let full_cmd = format!("{}\r\n", cmd);
    port.write_all(full_cmd.as_bytes())
        .map_err(|e| format!("Write failed on {}: {}", port_path, e))?;
    port.flush().ok();

    std::thread::sleep(Duration::from_millis(150));

    let mut response = Vec::new();
    let mut read_buf = [0u8; 2048];
    let start_time = std::time::Instant::now();

    while start_time.elapsed() < Duration::from_millis(timeout_ms) {
        match port.read(&mut read_buf) {
            Ok(n) if n > 0 => {
                response.extend_from_slice(&read_buf[..n]);
                let s = String::from_utf8_lossy(&response);
                if s.contains("OK\r") || s.contains("OK\n") || s.contains("NG") || s.contains("ERROR") {
                    break;
                }
            }
            _ => {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }

    Ok(String::from_utf8_lossy(&response).to_string())
}

pub fn query_device_token(port_path: &str) -> Option<ParsedTokenInfo> {
    let res9 = send_at(port_path, "AT+ENGMODES=9,0", 700).ok()?;
    if !res9.contains("+ENGMODES:9,OK,") {
        return None;
    }
    let res1 = send_at(port_path, "AT+ENGMODES=1,1,0", 500).ok();
    parse_engmodes_info(&res9, res1.as_deref())
}

pub fn remove_token_offline(port_path: &str) -> Result<String, String> {
    let res = send_at(port_path, "AT+ENGMODES=0,0,3,0", 1500)?;
    if res.contains("OK") || res.contains("successfully removed") {
        Ok("Token removed offline successfully".to_string())
    } else {
        Err(format!("Device returned non-OK: {}", res.trim()))
    }
}

// --- Standalone Persistent Token Store (90 Days Expiration) ---
use chrono::Utc;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

const TOKEN_STORE_PATH: &str = "/var/lib/mrt-station/tokens.json";
const FALLBACK_TOKEN_STORE: &str = "/tmp/mrt_station_tokens.json";

fn get_store_path() -> PathBuf {
    if let Ok(dir) = std::env::var("HOME") {
        let p = PathBuf::from(dir).join(".mrt-station");
        if fs::create_dir_all(&p).is_ok() {
            return p.join("tokens.json");
        }
    }
    let p = PathBuf::from(TOKEN_STORE_PATH);
    if let Some(parent) = p.parent() {
        if fs::create_dir_all(parent).is_ok() {
            return p;
        }
    }
    PathBuf::from(FALLBACK_TOKEN_STORE)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredToken {
    pub serial: String,
    pub token_id: String,
    pub model: String,
    pub owner: String,
    pub issued: String,
    pub expires: String,
    pub status: String,
    pub active_modes: Vec<String>,
    pub quotas: Vec<ModeQuota>,
}

pub fn load_token_store() -> HashMap<String, StoredToken> {
    let path = get_store_path();
    if let Ok(data) = fs::read_to_string(&path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        HashMap::new()
    }
}

pub fn save_token_store(store: &HashMap<String, StoredToken>) {
    let path = get_store_path();
    if let Ok(json_str) = serde_json::to_string_pretty(store) {
        let _ = fs::write(path, json_str);
    }
}

pub fn save_token_to_store(token: &StoredToken) {
    let mut store = load_token_store();
    store.insert(token.serial.clone(), token.clone());
    save_token_store(&store);
}

pub fn register_installed_token(
    serial: &str,
    model: &str,
    expiry_days: i64,
    selected_modes: Option<Vec<i32>>,
) -> StoredToken {
    let mut store = load_token_store();
    let now = Utc::now();
    let issued = now.format("%Y-%m-%d").to_string();
    let expires = (now + chrono::Duration::days(expiry_days)).format("%Y-%m-%d").to_string();
    let token_id = format!("MRT-{}", &Uuid::new_v4().to_string().replace("-", "").to_uppercase()[..16]);

    let modes = match selected_modes {
        Some(m) if !m.is_empty() => m,
        _ => vec![MODE_ENG_KERNEL, MODE_INIT_EM, MODE_ACCESS_SOD, MODE_EXTRACT_RAM_DUMP],
    };

    let mut active_modes = Vec::new();
    let mut quotas = Vec::new();

    for idx in modes {
        let name = mode_index_to_name(idx);
        if name != "MODE_UNKNOWN" {
            active_modes.push(name.to_string());
            let tuc = match idx {
                MODE_ENG_KERNEL => 10,
                MODE_ACCESS_SOD => 100,
                MODE_INIT_EM => 1000,
                MODE_EXTRACT_RAM_DUMP => 1000,
                _ => 100,
            };
            quotas.push(ModeQuota {
                index: idx,
                name: name.to_string(),
                tuc,
            });
        }
    }

    let token = StoredToken {
        serial: serial.to_string(),
        token_id,
        model: model.to_string(),
        owner: "endri.s".to_string(),
        issued,
        expires,
        status: "ACTIVE".to_string(),
        active_modes,
        quotas,
    };

    store.insert(serial.to_string(), token.clone());
    save_token_store(&store);
    token
}

pub fn unregister_token(serial: &str) {
    let mut store = load_token_store();
    store.remove(serial);
    save_token_store(&store);
}

fn format_date(raw: &str) -> Option<String> {
    if raw.len() == 8 {
        Some(format!("{}-{}-{}", &raw[0..4], &raw[4..6], &raw[6..8]))
    } else {
        Some(raw.to_string())
    }
}

pub fn parse_engmodes_info(res9: &str, res1: Option<&str>) -> Option<ParsedTokenInfo> {
    let mut info = ParsedTokenInfo::default();

    for line in res9.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("+ENGMODES:9,OK,") {
            let parts: Vec<&str> = trimmed.split(',').collect();
            if parts.len() >= 11 {
                info.token_id = Some(parts[3].to_string());
                info.model = Some(parts[4].to_string());
                info.owner = Some(parts[5].to_string());
                info.issued = format_date(parts[6]);
                info.expires = format_date(parts[7]);
                info.status = "ACTIVE".to_string();

                let quota_str = parts[10];
                for q in quota_str.split('_') {
                    let kv: Vec<&str> = q.split(':').collect();
                    if kv.len() == 2 {
                        if let (Ok(idx), Ok(tuc)) = (kv[0].parse::<i32>(), kv[1].parse::<i32>()) {
                            info.quotas.push(ModeQuota {
                                index: idx,
                                name: mode_index_to_name(idx).to_string(),
                                tuc,
                            });
                        }
                    }
                }
            }
            break;
        }
    }

    if info.token_id.is_none() {
        return None;
    }

    if let Some(r1) = res1 {
        for line in r1.lines() {
            let trimmed = line.trim();
            if trimmed.contains("TOK,") {
                if let Some(tok_part) = trimmed.split("TOK,").nth(1) {
                    for idx_str in tok_part.split('_') {
                        if let Ok(idx) = idx_str.trim().parse::<i32>() {
                            info.active_modes.push(mode_index_to_name(idx).to_string());
                        }
                    }
                }
            }
        }
    }

    if info.active_modes.is_empty() && !info.quotas.is_empty() {
        info.active_modes = info.quotas.iter().map(|q| q.name.clone()).collect();
    }

    Some(info)
}

pub fn inject_signed_token(port_path: &str, signed_payload: &str) -> Result<String, String> {
    let cmd = format!("AT+ENGMODES=0,0,1,{}", signed_payload);
    let res = send_at(port_path, &cmd, 3000)?;
    if res.contains("OK") {
        Ok("Injected".into())
    } else {
        Err(format!("HW rejected: {}", res.trim()))
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_engmodes_device_response() {
        let raw9 = "+ENGMODES:9,OK,01,2628701844800F74,SM-F971B,endri.s,20261014,20261128,20261128,D,0:10_12:100_1:695_49:999\n\nOK";
        let raw1 = "+ENGMODES:1,OK,TOK,0_12_1_49\n\nOK";
        let info = parse_engmodes_info(raw9, Some(raw1)).expect("Should parse token info");

        assert_eq!(info.token_id.as_deref(), Some("2628701844800F74"));
        assert_eq!(info.owner.as_deref(), Some("endri.s"));
        assert_eq!(info.expires.as_deref(), Some("2026-11-28"));
        assert_eq!(info.model.as_deref(), Some("SM-F971B"));
        assert_eq!(info.status, "ACTIVE");
        assert_eq!(info.active_modes.len(), 4);
        assert_eq!(info.quotas.len(), 4);
    }
}

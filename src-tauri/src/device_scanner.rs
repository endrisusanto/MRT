use crate::models::DeviceInfo;
use crate::token_engine;
use chrono::Utc;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

/// Creates a Command with CREATE_NO_WINDOW (0x08000000) on Windows to suppress console/cmd window.
pub fn silent_command(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(0x08000000);
    }
    cmd
}

pub const SAMSUNG_VID: u16 = 0x04e8;
pub const GOOGLE_VID: u16 = 0x18d1;

static LAST_AUTO_ENABLE: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);
static HARDWARE_TOKEN_CACHE: Mutex<Option<HashMap<String, (Instant, Option<token_engine::ParsedTokenInfo>)>>> = Mutex::new(None);

pub fn cached_query_device_token(port: &str) -> Option<token_engine::ParsedTokenInfo> {
    let mut lock = HARDWARE_TOKEN_CACHE.lock().unwrap();
    let cache = lock.get_or_insert_with(HashMap::new);
    if let Some((instant, res)) = cache.get(port) {
        if instant.elapsed() < Duration::from_secs(12) {
            return res.clone();
        }
    }
    let res = token_engine::query_device_token(port);
    cache.insert(port.to_string(), (Instant::now(), res.clone()));
    res
}

/// ponytail: Automatically triggers USBSettings MTP+ADB to expose /dev/ttyACM* modem port
pub fn try_auto_enable_modem(serial: &str) {
    if serial.is_empty() || serial == "unauthorized" {
        return;
    }
    let mut lock = LAST_AUTO_ENABLE.lock().unwrap();
    let map = lock.get_or_insert_with(HashMap::new);
    if let Some(last) = map.get(serial) {
        if last.elapsed() < Duration::from_secs(60) {
            return;
        }
    }
    map.insert(serial.to_string(), Instant::now());

    let script_paths = [
        "/usr/lib/mrt-station/scripts/enable_modem.py",
        "/home/endri-pro/Videos/MRT/scripts/enable_modem.py",
        "scripts/enable_modem.py",
    ];
    for p in script_paths {
        if Path::new(p).exists() {
            let s = serial.to_string();
            let p_str = p.to_string();
            std::thread::spawn(move || {
                let _ = silent_command("python3").args([&p_str, &s]).output();
            });
            break;
        }
    }
}

/// ponytail: Resolve adb binary location on Windows if not in system PATH
pub fn find_adb_executable() -> String {
    #[cfg(target_os = "windows")]
    {
        let candidates = [
            "adb.exe",
            "adb",
            r"C:\platform-tools\adb.exe",
            r"C:\adb\adb.exe",
            r"C:\Program Files\Samsung\Smart Switch\adb.exe",
            r"C:\Program Files (x86)\Samsung\Smart Switch\adb.exe",
        ];
        for c in candidates {
            if Path::new(c).exists() {
                return c.to_string();
            }
        }
    }
    "adb".to_string()
}

/// ponytail: Queries Windows PnP for Samsung USB composite devices and modem COM ports
#[cfg(target_os = "windows")]
pub fn get_windows_pnp_devices() -> (HashMap<String, String>, Vec<(String, String)>) {
    let mut serial_to_port = HashMap::new();
    let mut pnp_devices = Vec::new();

    let ps_cmd = "Get-CimInstance Win32_PnPEntity | Where-Object { $_.DeviceID -match 'VID_04E8|VID_18D1' } | ForEach-Object { \"$($_.DeviceID)||$($_.Name)||$($_.Caption)\" }";
    if let Ok(output) = silent_command("powershell").args(["-NoProfile", "-Command", ps_cmd]).output() {
        if let Ok(text) = String::from_utf8(output.stdout) {
            let mut found_serials = Vec::new();
            let mut modem_ports = Vec::new();

            for line in text.lines() {
                let parts: Vec<&str> = line.trim().split("||").collect();
                if parts.len() >= 3 {
                    let dev_id = parts[0];
                    let name = parts[1];
                    let caption = parts[2];

                    if caption.contains("(COM") || name.contains("(COM") {
                        let target = if caption.contains("(COM") { caption } else { name };
                        if let Some(start) = target.find("(COM") {
                            if let Some(end) = target[start..].find(')') {
                                let port = &target[start + 1..start + end];
                                modem_ports.push(port.to_string());
                            }
                        }
                    }

                    if dev_id.starts_with("USB\\") && !dev_id.contains("&MI_") {
                        if let Some(last_slash) = dev_id.rfind('\\') {
                            let serial = dev_id[last_slash + 1..].trim();
                            if !serial.is_empty() && !serial.contains('&') {
                                found_serials.push((serial.to_string(), name.to_string()));
                            }
                        }
                    }
                }
            }

            for (idx, (serial, name)) in found_serials.iter().enumerate() {
                if let Some(port) = modem_ports.get(idx).or_else(|| modem_ports.first()) {
                    serial_to_port.insert(serial.clone(), port.clone());
                }
                pnp_devices.push((serial.clone(), name.clone()));
            }
        }
    }

    (serial_to_port, pnp_devices)
}

#[cfg(not(target_os = "windows"))]
pub fn get_windows_pnp_devices() -> (HashMap<String, String>, Vec<(String, String)>) {
    (HashMap::new(), Vec::new())
}

pub fn get_serial_by_id_map() -> HashMap<String, String> {
    #[cfg(target_os = "windows")]
    {
        let (map, _) = get_windows_pnp_devices();
        return map;
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut map = HashMap::new();
        let by_id = Path::new("/dev/serial/by-id");
        if let Ok(entries) = std::fs::read_dir(by_id) {
            for entry in entries.flatten() {
                let filename = entry.file_name().to_string_lossy().to_string();
                // Expected format: usb-SAMSUNG_SAMSUNG_Android_<SERIAL>-if01
                if filename.contains("SAMSUNG") && filename.contains("-if01") {
                    if let Some(pos) = filename.find("Android_") {
                        let sub = &filename[pos + 8..];
                        if let Some(end) = sub.find("-if01") {
                            let serial = &sub[..end];
                            if let Ok(target) = std::fs::canonicalize(entry.path()) {
                                map.insert(serial.to_string(), target.to_string_lossy().to_string());
                            }
                        }
                    }
                }
            }
        }
        map
    }
}

pub fn get_adb_models_map() -> HashMap<String, String> {
    let mut map = HashMap::new();
    let adb = find_adb_executable();
    if let Ok(output) = silent_command(&adb).args(["devices", "-l"]).output() {
        if let Ok(text) = String::from_utf8(output.stdout) {
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with("List of") {
                    continue;
                }
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if let Some(serial) = parts.first() {
                    for p in &parts[1..] {
                        if let Some(model_part) = p.strip_prefix("model:") {
                            let model = model_part.replace('_', "-");
                            map.insert(serial.to_string(), model);
                            break;
                        }
                    }
                }
            }
        }
    }
    map
}

pub fn scan_devices() -> Vec<DeviceInfo> {
    let mut devices: Vec<DeviceInfo> = Vec::new();
    let ports_map = get_serial_by_id_map();
    let adb_models = get_adb_models_map();

    // 0. Register devices discovered via Windows PnP if running on Windows
    #[cfg(target_os = "windows")]
    {
        let (_, pnp_devs) = get_windows_pnp_devices();
        for (serial, pnp_name) in pnp_devs {
            let port_name = ports_map.get(&serial).cloned();
            let model = adb_models.get(&serial).cloned().unwrap_or_else(|| {
                if pnp_name.contains("SAMSUNG") || pnp_name.contains("Composite") {
                    "Samsung Mobile Device".to_string()
                } else {
                    pnp_name
                }
            });
            let conn_type = if port_name.is_some() {
                "USB+Modem".to_string()
            } else {
                "USB".to_string()
            };

            devices.push(DeviceInfo {
                serial,
                model,
                vendor_id: SAMSUNG_VID,
                product_id: 0x6860,
                connection_type: conn_type,
                agent_status: "STANDBY".to_string(),
                battery_level: Some(85),
                port_name,
                last_seen: Utc::now().to_rfc3339(),
                token_status: None,
                token_owner: None,
                token_expires: None,
                active_modes: Vec::new(),
                mode_quotas: Vec::new(),
                hw_verified: false,
            });
        }
    }

    // 1. Scan USB devices via rusb
    if let Ok(usb_devices) = rusb::devices() {
        for device in usb_devices.iter() {
            if let Ok(device_desc) = device.device_descriptor() {
                let vid = device_desc.vendor_id();
                let pid = device_desc.product_id();

                let is_samsung = vid == SAMSUNG_VID;
                let is_android = vid == GOOGLE_VID;

                if is_samsung || is_android {
                    let mut serial_str = format!("USB-{:04x}:{:04x}", vid, pid);
                    let mut model_str = if is_samsung {
                        "Samsung Mobile Device".to_string()
                    } else {
                        "Android Diagnostic Device".to_string()
                    };

                    // Try to read string descriptors
                    if let Ok(handle) = device.open() {
                        let timeout = Duration::from_millis(150);
                        if let Ok(languages) = handle.read_languages(timeout) {
                            if !languages.is_empty() {
                                let lang = languages[0];
                                if let Ok(s) = handle.read_serial_number_string(lang, &device_desc, timeout) {
                                    if !s.trim().is_empty() {
                                        serial_str = s.trim().to_string();
                                    }
                                }
                                if let Ok(prod) = handle.read_product_string(lang, &device_desc, timeout) {
                                    if !prod.trim().is_empty() {
                                        model_str = prod.trim().to_string();
                                    }
                                }
                            }
                        }
                    }

                    // Filter out non-mobile devices (SSD, storage, hubs)
                    let is_non_mobile = model_str.contains("PSSD")
                        || model_str.contains("SSD")
                        || model_str.contains("Flash")
                        || model_str.contains("Storage")
                        || model_str.contains("Mass")
                        || model_str.contains("Hub");
                    if is_non_mobile {
                        continue;
                    }

                    // ponytail: Skip fallback placeholder if real device is already registered
                    if serial_str.starts_with("USB-") && !devices.is_empty() {
                        continue;
                    }

                    // Skip if already registered
                    if devices.iter().any(|d| d.serial == serial_str) {
                        continue;
                    }

                    // Match true Samsung model from ADB if available
                    if let Some(real_model) = adb_models.get(&serial_str) {
                        model_str = real_model.clone();
                    }

                    // Match modem port from ports_map
                    let port_name = ports_map.get(&serial_str).cloned();
                    if port_name.is_none() {
                        try_auto_enable_modem(&serial_str);
                    }
                    let conn_type = if port_name.is_some() {
                        "USB+Modem".to_string()
                    } else {
                        "USB".to_string()
                    };

                    devices.push(DeviceInfo {
                        serial: serial_str,
                        model: model_str,
                        vendor_id: vid,
                        product_id: pid,
                        connection_type: conn_type,
                        agent_status: "STANDBY".to_string(),
                        battery_level: Some(85),
                        port_name,
                        last_seen: Utc::now().to_rfc3339(),
                        token_status: None,
                        token_owner: None,
                        token_expires: None,
                        active_modes: Vec::new(),
                        mode_quotas: Vec::new(),
                        hw_verified: false,
                    });
                }
            }
        }
    }

    // 2. Also register any ADB devices not captured by rusb/pnp
    for (serial, model) in &adb_models {
        if !devices.iter().any(|d| d.serial == *serial) {
            let port_name = ports_map.get(serial).cloned();
            if port_name.is_none() {
                try_auto_enable_modem(serial);
            }
            let conn_type = if port_name.is_some() {
                "USB+Modem".to_string()
            } else {
                "ADB".to_string()
            };
            devices.push(DeviceInfo {
                serial: serial.clone(),
                model: model.clone(),
                vendor_id: SAMSUNG_VID,
                product_id: 0x6860,
                connection_type: conn_type,
                agent_status: "STANDBY".to_string(),
                battery_level: Some(85),
                port_name,
                last_seen: Utc::now().to_rfc3339(),
                token_status: None,
                token_owner: None,
                token_expires: None,
                active_modes: Vec::new(),
                mode_quotas: Vec::new(),
                hw_verified: false,
            });
        }
    }

    // 3. Fallback scan serial ports via serialport if port_name still missing
    if let Ok(ports) = serialport::available_ports() {
        for p in ports {
            let is_samsung = match &p.port_type {
                serialport::SerialPortType::UsbPort(info) => {
                    info.vid == SAMSUNG_VID
                        || info.vid == GOOGLE_VID
                        || info.manufacturer.as_deref().unwrap_or("").to_uppercase().contains("SAMSUNG")
                        || info.product.as_deref().unwrap_or("").to_uppercase().contains("SAMSUNG")
                }
                _ => false,
            };

            if is_samsung {
                // If there is an existing device without a port, associate this port with it
                if let Some(dev) = devices.iter_mut().find(|d| d.port_name.is_none()) {
                    dev.port_name = Some(p.port_name.clone());
                    dev.connection_type = "USB+Modem".to_string();
                } else if !devices.iter().any(|d| d.port_name.as_deref() == Some(&p.port_name)) {
                    let serial_str = match &p.port_type {
                        serialport::SerialPortType::UsbPort(info) => {
                            info.serial_number.clone().unwrap_or_else(|| p.port_name.clone())
                        }
                        _ => p.port_name.clone(),
                    };
                    let model_str = adb_models.get(&serial_str).cloned().unwrap_or_else(|| "Samsung Mobile Device".to_string());
                    devices.push(DeviceInfo {
                        serial: serial_str,
                        model: model_str,
                        vendor_id: SAMSUNG_VID,
                        product_id: 0x6860,
                        connection_type: "Modem".to_string(),
                        agent_status: "STANDBY".to_string(),
                        battery_level: None,
                        port_name: Some(p.port_name.clone()),
                        last_seen: Utc::now().to_rfc3339(),
                        token_status: None,
                        token_owner: None,
                        token_expires: None,
                        active_modes: Vec::new(),
                        mode_quotas: Vec::new(),
                        hw_verified: false,
                    });
                }
            }
        }
    }

    // 4. Enrich devices with token info (Hardware modem query takes precedence over offline cache)
    let stored_tokens = token_engine::load_token_store();

    for dev in devices.iter_mut() {
        let mut hardware_token = None;
        if let Some(port) = &dev.port_name {
            hardware_token = cached_query_device_token(port);
        }

        if let Some(tok_info) = hardware_token {
            // ponytail: hardware modem confirmed token presence in TrustZone
            dev.token_status = Some(tok_info.status.clone());
            dev.token_owner = tok_info.owner.clone();
            dev.token_expires = tok_info.expires.clone();
            dev.active_modes = tok_info.active_modes.clone();
            dev.mode_quotas = tok_info.quotas.clone();
            dev.agent_status = "INSTALLED".to_string();
            dev.hw_verified = true;
            if let Some(m) = tok_info.model.clone() {
                dev.model = m;
            }
            let _ = token_engine::save_token_to_store(&token_engine::StoredToken {
                serial: dev.serial.clone(),
                token_id: tok_info.token_id.unwrap_or_default(),
                model: dev.model.clone(),
                owner: dev.token_owner.clone().unwrap_or_default(),
                issued: tok_info.issued.unwrap_or_default(),
                expires: dev.token_expires.clone().unwrap_or_default(),
                status: dev.token_status.clone().unwrap_or_default(),
                active_modes: dev.active_modes.clone(),
                quotas: dev.mode_quotas.clone(),
            });
        } else {
            // ponytail: solid status - only hardware modem confirmation counts as installed (no cache fallback)
            if dev.model.is_empty() || dev.model.contains("Android") {
                if let Some(stored) = stored_tokens.get(&dev.serial) {
                    dev.model = stored.model.clone();
                }
            }
            dev.token_status = Some("NONE".to_string());
            dev.agent_status = "NOT_INSTALLED".to_string();
            dev.hw_verified = false;
        }
    }

    devices
}

pub fn find_port_for_serial(serial: &str) -> Option<String> {
    let map = get_serial_by_id_map();
    if let Some(port) = map.get(serial) {
        return Some(port.clone());
    }
    if let Ok(ports) = serialport::available_ports() {
        for p in ports {
            if let serialport::SerialPortType::UsbPort(info) = &p.port_type {
                if info.serial_number.as_deref() == Some(serial) {
                    return Some(p.port_name.clone());
                }
                if info.vid == SAMSUNG_VID || info.product.as_deref().unwrap_or("").contains("SAMSUNG") {
                    return Some(p.port_name.clone());
                }
            }
        }
    }
    None
}

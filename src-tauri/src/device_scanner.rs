use crate::models::DeviceInfo;
use chrono::Utc;
use std::time::Duration;

pub const SAMSUNG_VID: u16 = 0x04e8;
pub const GOOGLE_VID: u16 = 0x18d1;

pub fn scan_devices() -> Vec<DeviceInfo> {
    let mut devices = Vec::new();

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

                    // Try to read string descriptors if device handle can be opened
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

                    devices.push(DeviceInfo {
                        serial: serial_str,
                        model: model_str,
                        vendor_id: vid,
                        product_id: pid,
                        connection_type: "USB".to_string(),
                        agent_status: "INSTALLED".to_string(),
                        battery_level: Some(85),
                        port_name: None,
                        last_seen: Utc::now().to_rfc3339(),
                    });
                }
            }
        }
    }

    // 2. Scan serial ports via serialport
    if let Ok(ports) = serialport::available_ports() {
        for p in ports {
            match p.port_type {
                serialport::SerialPortType::UsbPort(info) => {
                    let vid = info.vid;
                    let pid = info.pid;
                    let is_target = vid == SAMSUNG_VID || vid == GOOGLE_VID;
                    if is_target {
                        let serial_str = info.serial_number.unwrap_or_else(|| p.port_name.clone());
                        let model_str = info.product.unwrap_or_else(|| "Samsung Diagnostic Port".to_string());

                        // Deduplicate if already found via rusb
                        if !devices.iter().any(|d| d.serial == serial_str || d.port_name.as_deref() == Some(&p.port_name)) {
                            devices.push(DeviceInfo {
                                serial: serial_str,
                                model: model_str,
                                vendor_id: vid,
                                product_id: pid,
                                connection_type: "Serial".to_string(),
                                agent_status: "INSTALLED".to_string(),
                                battery_level: None,
                                port_name: Some(p.port_name),
                                last_seen: Utc::now().to_rfc3339(),
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    devices
}

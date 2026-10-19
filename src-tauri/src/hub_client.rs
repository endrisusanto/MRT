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

    let auth_for_worker = auth_manager.clone();
    let hub_for_worker = hub_url.clone();
    tokio::spawn(async move {
        start_relay_worker_loop(hub_for_worker, auth_for_worker).await;
    });

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

async fn execute_relay_task(
    task_type: &str,
    payload: &serde_json::Value,
    auth_manager: &crate::auth_manager::AuthManager,
) -> serde_json::Value {
    let kms_url = {
        let lock = auth_manager.config.read().unwrap();
        lock.kms_endpoint.clone()
    };
    let ip = local_ip().map(|i| i.to_string()).unwrap_or_else(|_| "127.0.0.1".to_string());

    match task_type {
        "PPC_CHALLENGE" => {
            let pss_payload = payload.get("pss_payload").and_then(|v| v.as_str()).unwrap_or("");
            let cid = match crate::kms_flow::extract_pss_cid(pss_payload) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[Relay Worker] Extract CID failed: {}", e);
                    return serde_json::json!({
                        "success": false,
                        "error": format!("Ekstraksi CID gagal: {}", e)
                    });
                }
            };

            let ppc_res = match crate::kms_flow::request_kms_ppc(&kms_url, &cid, &ip, "00:00:00:00:00:00").await {
                Ok(res) => res,
                Err(e) => {
                    eprintln!("[Relay Worker] REQ-PPC failed: {}", e);
                    return serde_json::json!({
                        "success": false,
                        "error": format!("KMS REQ-PPC gagal di Relay Agent: {}", e)
                    });
                }
            };

            let cert_b64 = match ppc_res.cert_b64 {
                Some(c) if !c.is_empty() => c,
                _ => {
                    return serde_json::json!({
                        "success": false,
                        "error": format!("KMS REQ-PPC ditolak server Samsung dengan retCode: {}", ppc_res.ret_code)
                    });
                }
            };

            let modes: Vec<i32> = payload.get("modes")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|x| x.as_i64().map(|n| n as i32)).collect())
                .unwrap_or_else(|| vec![0, 1, 12, 49]);
            let user_id = payload.get("user_id").and_then(|v| v.as_str()).unwrap_or("endri.s");

            match crate::kms_flow::construct_pit_challenge_command(&cert_b64, &modes, user_id, None) {
                Ok(cmd) => serde_json::json!({
                    "success": true,
                    "challenge_cmd": cmd,
                    "cert_b64": cert_b64,
                    "cert_len": cert_b64.len()
                }),
                Err(e) => serde_json::json!({
                    "success": false,
                    "error": format!("Gagal menyusun perintah challenge PPC: {}", e)
                }),
            }
        }
        "REQUEST_TOKEN" => {
            let serial = payload.get("serial").and_then(|v| v.as_str()).unwrap_or("");
            let model = payload.get("model").and_then(|v| v.as_str()).unwrap_or("");
            let raw_nonce = payload.get("device_nonce").and_then(|v| v.as_str()).unwrap_or("");
            let nonce_obj = match crate::kms_flow::parse_modem_pit_response(raw_nonce) {
                Ok(n) => n,
                Err(e) => {
                    return serde_json::json!({
                        "success": false,
                        "error": format!("Payload Device Nonce tidak valid: {}", e)
                    });
                }
            };

            let auth_status = auth_manager.get_status();
            let ad_token = auth_status.username.unwrap_or_else(|| "endri.s".to_string());
            let cid = format!("MRT-PIT-{}", uuid::Uuid::new_v4().to_string().replace("-", ""));

            match crate::kms_flow::request_kms_pit(
                &kms_url,
                &ad_token,
                &nonce_obj,
                &cid,
                &ip,
                "000000000000",
            ).await {
                Ok(pit_res) => {
                    serde_json::json!({
                        "success": true,
                        "serial": serial,
                        "model": model,
                        "signed_payload": pit_res.e_token_b64.unwrap_or_default(),
                        "rxss_iv": pit_res.rxss_iv_b64,
                        "message": "Token signed successfully by Samsung KMS via Relay Worker"
                    })
                }
                Err(e) => {
                    eprintln!("[Relay Worker] REQ-PIT error: {}", e);
                    serde_json::json!({
                        "success": false,
                        "error": format!("KMS REQ-PIT gagal: {}", e)
                    })
                }
            }
        }
        _ => serde_json::json!({
            "success": false,
            "error": format!("Unknown task type: {}", task_type)
        }),
    }
}

pub async fn start_relay_worker_loop(
    hub_url: String,
    auth_manager: crate::auth_manager::AuthManager,
) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(35))
        .build()
        .unwrap_or_default();

    let poll_endpoint = format!("{}/api/relay/poll", hub_url.trim_end_matches('/'));
    let complete_endpoint = format!("{}/api/relay/complete", hub_url.trim_end_matches('/'));

    loop {
        if !auth_manager.is_relay() {
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        }

        let res = match client.get(&poll_endpoint).send().await {
            Ok(r) => r,
            Err(_) => {
                tokio::time::sleep(Duration::from_secs(3)).await;
                continue;
            }
        };

        if !res.status().is_success() {
            tokio::time::sleep(Duration::from_secs(3)).await;
            continue;
        }

        let tasks: Vec<serde_json::Value> = res.json().await.unwrap_or_default();
        for task in tasks {
            let task_id = task.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let task_type = task.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let payload = task.get("payload").cloned().unwrap_or(serde_json::Value::Null);

            let result = execute_relay_task(&task_type, &payload, &auth_manager).await;

            let _ = client
                .post(&complete_endpoint)
                .json(&serde_json::json!({
                    "taskId": task_id,
                    "result": result
                }))
                .send()
                .await;
        }
    }
}

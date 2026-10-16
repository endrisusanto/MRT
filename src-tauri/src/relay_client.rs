use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct RelayRequest {
    pub serial: String,
    pub model: String,
    pub modes: Vec<i32>,
    #[serde(default)]
    pub device_nonce: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RelayResponse {
    pub success: bool,
    pub signed_payload: Option<String>,
    pub message: Option<String>,
}

/// Check if an active relay agent node is online in Central Hub
pub async fn is_relay_active_on_hub(hub_url: &str) -> Result<bool, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("{}/api/nodes", hub_url.trim_end_matches('/'));
    let res = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => return Err(format!("Central Hub tidak dapat dijangkau ({}): {}", url, e)),
    };

    if !res.status().is_success() {
        return Ok(false);
    }

    let nodes: Vec<serde_json::Value> = res.json().await.unwrap_or_default();
    let has_relay = nodes.iter().any(|n| {
        let is_relay = n.get("is_relay").and_then(|v| v.as_bool()).unwrap_or(false);
        let status = n.get("status").and_then(|v| v.as_str()).unwrap_or("");
        is_relay && (status == "ONLINE" || status.is_empty())
    });

    Ok(has_relay)
}

/// Request PPC challenge hash from Relay Agent via Central Hub
pub async fn request_ppc_challenge(
    hub_url: &str,
    pss_payload: &str,
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("{}/api/relay/ppc-challenge", hub_url.trim_end_matches('/'));
    let res = client
        .post(&url)
        .json(&serde_json::json!({ "pss_payload": pss_payload }))
        .send()
        .await
        .map_err(|e| format!("Gagal menghubungi Relay untuk PPC challenge ({}): {}", url, e))?;

    if !res.status().is_success() {
        let err_body: serde_json::Value = res.json().await.unwrap_or_default();
        let msg = err_body
            .get("message")
            .or_else(|| err_body.get("error"))
            .and_then(|v| v.as_str())
            .unwrap_or("Relay Agent menolak permintaan PPC challenge");
        return Err(msg.to_string());
    }

    let body: serde_json::Value = res.json().await
        .map_err(|e| format!("Invalid JSON response for PPC challenge: {}", e))?;

    body.get("ppc_hash")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "Respons Relay Agent tidak menyertakan ppc_hash".to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedTokenData {
    pub signed_payload: String,
    pub rxss_iv: Option<String>,
}

/// Request full signed token payload and IV from Relay Agent
pub async fn request_signed_token_full(
    relay_url: &str,
    serial: &str,
    model: &str,
    modes: &[i32],
    device_nonce: Option<&str>,
) -> Result<SignedTokenData, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let url = if relay_url.ends_with("/request-token") {
        relay_url.to_string()
    } else {
        format!("{}/api/relay/request-token", relay_url.trim_end_matches('/'))
    };
    let req = RelayRequest {
        serial: serial.to_string(),
        model: model.to_string(),
        modes: modes.to_vec(),
        device_nonce: device_nonce.map(|s| s.to_string()),
    };

    let res = client
        .post(&url)
        .json(&req)
        .send()
        .await
        .map_err(|e| format!("Central Hub / Relay tidak dapat dihubungi ({}): {}", url, e))?;

    if !res.status().is_success() {
        let status = res.status();
        let body_json: serde_json::Value = res.json().await.unwrap_or_default();
        let err_msg = body_json
            .get("message")
            .or_else(|| body_json.get("error"))
            .and_then(|v| v.as_str())
            .unwrap_or("Relay menolak permintaan sign token");

        if status == reqwest::StatusCode::SERVICE_UNAVAILABLE || status.as_u16() == 503 {
            return Err(format!("Tidak ada Relay Agent yang aktif di Central Hub: {}", err_msg));
        }
        return Err(format!("Relay error (HTTP {}): {}", status, err_msg));
    }

    let body: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("Invalid JSON response from relay: {}", e))?;

    if let Some(payload) = body.get("signed_payload").and_then(|v| v.as_str()) {
        let rxss_iv = body.get("rxss_iv").and_then(|v| v.as_str()).map(|s| s.to_string());
        Ok(SignedTokenData {
            signed_payload: payload.to_string(),
            rxss_iv,
        })
    } else {
        let err_msg = body
            .get("error")
            .or_else(|| body.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("Relay response missing signed_payload");
        Err(err_msg.to_string())
    }
}

/// Request signed token payload from a Relay Agent instance.
pub async fn request_signed_token(
    relay_url: &str,
    serial: &str,
    model: &str,
    modes: &[i32],
    device_nonce: Option<&str>,
) -> Result<String, String> {
    request_signed_token_full(relay_url, serial, model, modes, device_nonce)
        .await
        .map(|d| d.signed_payload)
}

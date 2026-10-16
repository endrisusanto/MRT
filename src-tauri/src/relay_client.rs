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

/// Request signed token payload from a Relay Agent instance.
pub async fn request_signed_token(
    relay_url: &str,
    serial: &str,
    model: &str,
    modes: &[i32],
    device_nonce: Option<&str>,
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    // ponytail: automatically route to /api/relay/request-token
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
        Ok(payload.to_string())
    } else {
        let err_msg = body
            .get("error")
            .or_else(|| body.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("Relay response missing signed_payload");
        Err(err_msg.to_string())
    }
}

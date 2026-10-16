use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct RelayRequest {
    pub serial: String,
    pub model: String,
    pub modes: Vec<i32>,
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
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let url = format!("{}/relay/request-token", relay_url.trim_end_matches('/'));
    let req = RelayRequest {
        serial: serial.to_string(),
        model: model.to_string(),
        modes: modes.to_vec(),
    };

    let res = client
        .post(&url)
        .json(&req)
        .send()
        .await
        .map_err(|e| format!("Relay agent unreachable ({}): {}", url, e))?;

    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        return Err(format!("Relay agent returned HTTP {}: {}", status, body));
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

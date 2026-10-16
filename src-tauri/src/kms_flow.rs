// ponytail: Native Rust KMS protocol implementation for Samsung Knox Token Injection
use sha2::{Digest, Sha256};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PpcRequest {
    #[serde(rename = "msgType")]
    pub msg_type: String,
    #[serde(rename = "protVer")]
    pub prot_ver: String,
    pub cid: String,
    pub ip: String,
    pub mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PpcResponse {
    #[serde(rename = "retCode", default)]
    pub ret_code: String,
    #[serde(rename = "certB64", default)]
    pub cert_b64: Option<String>,
    #[serde(rename = "hashB64", default)]
    pub hash_b64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PitRequest {
    #[serde(rename = "msgType")]
    pub msg_type: String,
    #[serde(rename = "protVer")]
    pub prot_ver: String,
    #[serde(rename = "userType")]
    pub user_type: String,
    pub auth: String,
    #[serde(rename = "authLen")]
    pub auth_len: usize,
    pub serial: String,
    pub model: String,
    pub modes: Vec<i32>,
    #[serde(rename = "ermB64")]
    pub erm_b64: String,
    pub cid: String,
    pub ip: String,
    pub mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PitResponse {
    #[serde(rename = "retCode", default)]
    pub ret_code: String,
    #[serde(rename = "retMsg", default)]
    pub ret_msg: Option<String>,
    #[serde(rename = "eTokenB64", default)]
    pub e_token_b64: Option<String>,
    #[serde(rename = "eTokenB64Len", default)]
    pub e_token_b64_len: Option<usize>,
    #[serde(rename = "rxssIvB64", default)]
    pub rxss_iv_b64: Option<String>,
    #[serde(rename = "rxssIvB64Len", default)]
    pub rxss_iv_b64_len: Option<usize>,
}

/// Extract clean PS-S response line from AT+ENGMODES=1,3,1 output
pub fn extract_pss_payload(raw: &str) -> Result<String, String> {
    for line in raw.lines() {
        let t = line.trim();
        if t.starts_with("+ENGMODES:1,OK,") {
            return Ok(t.to_string());
        }
    }
    if raw.contains("OK") {
        return Ok(raw.trim().to_string());
    }
    Err(format!("Invalid PS-S response from modem: {}", raw.trim()))
}

/// Compute PPC Challenge Hash as implemented in JanusServiceAgent.dll
/// Hash = SHA256(string.Join(":", [parts[0]..parts[7], cert_hex_len, cert_hex]))
pub fn compute_ppc_hash(pss_payload: &str, cert_b64: &str) -> Result<String, String> {
    let cert_bytes = BASE64.decode(cert_b64.trim())
        .map_err(|e| format!("Failed to decode certB64: {}", e))?;
    
    // Hex string uppercase without dashes
    let cert_hex: String = cert_bytes.iter().map(|b| format!("{:02X}", b)).collect();
    let cert_len_str = cert_hex.len().to_string();

    // Split PS-S payload by ":"
    let mut parts: Vec<String> = pss_payload.split(':').map(|s| s.to_string()).collect();
    
    // Ensure array has at least 10 slots (indices 0..9)
    while parts.len() < 10 {
        parts.push(String::new());
    }
    parts[8] = cert_len_str;
    parts[9] = cert_hex;

    let challenge_str = parts.join(":");
    let mut hasher = Sha256::new();
    hasher.update(challenge_str.as_bytes());
    let result = hasher.finalize();

    let hash_hex: String = result.iter().map(|b| format!("{:02X}", b)).collect();
    Ok(hash_hex)
}

/// Construct injection command AT+ENGMODES=0,2,... as decompiled from JanusServiceAgent.dll
/// Format: AT+ENGMODES=0,2,:<iv_len>:<iv>:<token_len>:<token>:<sha256_hash>:
pub fn construct_injection_command(
    rxss_iv_b64: &str,
    e_token_b64: &str,
) -> String {
    let iv_clean = rxss_iv_b64.trim();
    let token_clean = e_token_b64.trim();
    let iv_len = iv_clean.len();
    let token_len = token_clean.len();

    let payload_body = format!(":{}:{}:{}:{}:", iv_len, iv_clean, token_len, token_clean);
    
    let mut hasher = Sha256::new();
    hasher.update(payload_body.as_bytes());
    let hash_result = hasher.finalize();
    let hash_hex: String = hash_result.iter().map(|b| format!("{:02X}", b)).collect();

    format!("AT+ENGMODES=0,2,{}{}:", payload_body, hash_hex)
}

/// Request PPC challenge from Samsung KMS server (runs inside intranet)
pub async fn request_kms_ppc(
    kms_endpoint: &str,
    cid: &str,
    client_ip: &str,
    client_mac: &str,
) -> Result<PpcResponse, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;

    let req = PpcRequest {
        msg_type: "REQ-PPC".to_string(),
        prot_ver: "04".to_string(),
        cid: cid.to_string(),
        ip: client_ip.to_string(),
        mac: client_mac.to_string(),
    };

    let url = kms_endpoint.trim_end_matches('/');
    let res = client
        .post(url)
        .json(&req)
        .send()
        .await
        .map_err(|e| format!("KMS REQ-PPC failed ({}): {}", url, e))?;

    if !res.status().is_success() {
        return Err(format!("KMS REQ-PPC returned HTTP {}", res.status()));
    }

    let ppc_res: PpcResponse = res.json().await
        .map_err(|e| format!("Failed to parse KMS PPC response: {}", e))?;

    if ppc_res.ret_code != "S" && ppc_res.cert_b64.is_none() {
        return Err(format!("KMS REQ-PPC rejected with code: {}", ppc_res.ret_code));
    }

    Ok(ppc_res)
}

/// Request token from Samsung KMS server using REQ-PIT nonce (runs inside intranet)
pub async fn request_kms_pit(
    kms_endpoint: &str,
    ad_token: &str,
    serial: &str,
    model: &str,
    modes: &[i32],
    erm_b64: &str,
    cid: &str,
    client_ip: &str,
    client_mac: &str,
) -> Result<PitResponse, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;

    let req = PitRequest {
        msg_type: "REQ-PIT".to_string(),
        prot_ver: "04".to_string(),
        user_type: "SEC".to_string(),
        auth: ad_token.to_string(),
        auth_len: ad_token.len(),
        serial: serial.to_string(),
        model: model.to_string(),
        modes: modes.to_vec(),
        erm_b64: erm_b64.to_string(),
        cid: cid.to_string(),
        ip: client_ip.to_string(),
        mac: client_mac.to_string(),
    };

    let url = kms_endpoint.trim_end_matches('/');
    let res = client
        .post(url)
        .json(&req)
        .send()
        .await
        .map_err(|e| format!("KMS REQ-PIT failed ({}): {}", url, e))?;

    if !res.status().is_success() {
        return Err(format!("KMS REQ-PIT returned HTTP {}", res.status()));
    }

    let pit_res: PitResponse = res.json().await
        .map_err(|e| format!("Failed to parse KMS PIT response: {}", e))?;

    if pit_res.ret_code != "S" && pit_res.e_token_b64.is_none() {
        let err_msg = pit_res.ret_msg.unwrap_or_else(|| pit_res.ret_code);
        return Err(format!("KMS REQ-PIT rejected: {}", err_msg));
    }

    Ok(pit_res)
}

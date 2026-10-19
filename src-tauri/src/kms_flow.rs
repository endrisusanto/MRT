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
pub struct ModemPitNonce {
    pub raw: String,
    pub esk_len: usize,
    pub esk_b64_len: usize,
    pub esk_b64: String,
    pub erm_len: usize,
    pub erm_b64_len: usize,
    pub erm_b64: String,
    pub hash_b64: String,
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
    #[serde(rename = "essId", default)]
    pub ess_id: String,
    pub ip: String,
    pub mac: String,
    #[serde(rename = "eeiLen", default)]
    pub eei_len: usize,
    #[serde(rename = "eeiB64Len", default)]
    pub eei_b64_len: usize,
    #[serde(rename = "eeiB64", default)]
    pub eei_b64: String,
    #[serde(rename = "eskLen")]
    pub esk_len: usize,
    #[serde(rename = "eskB64Len")]
    pub esk_b64_len: usize,
    #[serde(rename = "eskB64")]
    pub esk_b64: String,
    #[serde(rename = "ermLen")]
    pub erm_len: usize,
    #[serde(rename = "ermB64Len")]
    pub erm_b64_len: usize,
    #[serde(rename = "ermB64")]
    pub erm_b64: String,
    #[serde(rename = "hashB64")]
    pub hash_b64: String,
    pub cid: String,
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

/// Extract 8-character CID from AT+ENGMODES=1,3,1 modem output as decompiled from JanusServiceAgent.dll
pub fn extract_pss_cid(raw: &str) -> String {
    for line in raw.lines() {
        let t = line.trim();
        if t.contains(',') {
            let comma_parts: Vec<&str> = t.split(',').collect();
            if comma_parts.len() >= 3 {
                let part2 = comma_parts[2].trim();
                if part2.len() >= 8 {
                    return part2[..8].to_string();
                }
            }
        }
    }
    format!("{:08X}", uuid::Uuid::new_v4().as_u128() as u32)
}

/// Construct AT+ENGMODES=0,1,... challenge command as decompiled from JanusServiceAgent.dll
///
/// Command format:
/// AT+ENGMODES=0,1,04:<essId>:<modesCount>:<modesString>:<expiryDate>:<issueDate>:<userId>:000000:<certLen>:<certHex>:<hashHex>:
///
/// where <hashHex> = SHA256(UTF8("04:<essId>:<modesCount>:<modesString>:<expiryDate>:<issueDate>:<userId>:000000:<certLen>:<certHex>:"))
pub fn construct_pit_challenge_command(
    cert_b64: &str,
    modes: &[i32],
    user_id: &str,
    ess_id: Option<&str>,
) -> Result<String, String> {
    let cert_bytes = BASE64.decode(cert_b64.trim())
        .map_err(|e| format!("Failed to decode certB64: {}", e))?;
    let cert_hex: String = cert_bytes.iter().map(|b| format!("{:02X}", b)).collect();
    let cert_len_str = (cert_hex.len() / 2).to_string();

    let modes_str = modes.iter().map(|m| m.to_string()).collect::<Vec<_>>().join("|");
    let modes_count = modes.len().to_string();

    let now = chrono::Utc::now();
    let issue_date = now.format("%Y%m%d").to_string();
    let expiry_date = (now + chrono::Duration::days(45)).format("%Y%m%d").to_string();

    let user = if user_id.is_empty() { "endri.s" } else { user_id };
    let ess = ess_id.unwrap_or("");

    // 11 elements with empty string at the end ensures trailing colon when joined
    let parts = vec![
        "04",
        ess,
        &modes_count,
        &modes_str,
        &expiry_date,
        &issue_date,
        user,
        "000000",
        &cert_len_str,
        &cert_hex,
        "",
    ];

    let joined = parts.join(":");
    let mut hasher = Sha256::new();
    hasher.update(joined.as_bytes());
    let result = hasher.finalize();
    let hash_hex: String = result.iter().map(|b| format!("{:02X}", b)).collect();

    Ok(format!("AT+ENGMODES=0,1,{}{}:", joined, hash_hex))
}

/// Compute PPC Challenge Hash (legacy fallback)
pub fn compute_ppc_hash(pss_payload: &str, cert_b64: &str) -> Result<String, String> {
    let cert_bytes = BASE64.decode(cert_b64.trim())
        .map_err(|e| format!("Failed to decode certB64: {}", e))?;
    let cert_hex: String = cert_bytes.iter().map(|b| format!("{:02X}", b)).collect();
    let cert_len_str = (cert_hex.len() / 2).to_string();

    let mut parts: Vec<String> = pss_payload.split(':').map(|s| s.to_string()).collect();
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

/// Parse modem response to AT+ENGMODES=0,1,<hash> into structured ModemPitNonce
/// As decompiled from JanusServiceAgent.dll:
/// parts = response.Substring(3, length-3).Split(':')
/// parts[1]=eskLen, parts[2]=eskB64Len, parts[3]=eskB64,
/// parts[4]=ermLen, parts[5]=ermB64Len, parts[6]=ermB64, parts[7]=hashB64
pub fn parse_modem_pit_response(raw: &str) -> Result<ModemPitNonce, String> {
    for line in raw.lines() {
        let t = line.trim();

        // 1. If line contains "OK:" (standard modem response format +ENGMODES:0,OK:<eskLen>...)
        if let Some(ok_idx) = t.find("OK:") {
            let after_ok = &t[ok_idx + 3..];
            let parts: Vec<&str> = after_ok.split(':').collect();
            if parts.len() >= 7 {
                let esk_len = parts[0].parse::<usize>().unwrap_or(0);
                let esk_b64_len = parts[1].parse::<usize>().unwrap_or_else(|_| parts[2].len());
                let esk_b64 = parts[2].to_string();
                let erm_len = parts[3].parse::<usize>().unwrap_or(0);
                let erm_b64_len = parts[4].parse::<usize>().unwrap_or_else(|_| parts[5].len());
                let erm_b64 = parts[5].to_string();
                let hash_b64 = parts[6].trim().to_string();

                if !erm_b64.is_empty() {
                    return Ok(ModemPitNonce {
                        raw: t.to_string(),
                        esk_len,
                        esk_b64_len,
                        esk_b64,
                        erm_len,
                        erm_b64_len,
                        erm_b64,
                        hash_b64,
                    });
                }
            }
        }

        // 2. Direct parts[1..7] indexing from C# Substring(3)
        if t.contains(':') {
            let parts: Vec<&str> = t.split(':').collect();
            if parts.len() >= 8 {
                let esk_len = parts[1].parse::<usize>().unwrap_or(0);
                let esk_b64_len = parts[2].parse::<usize>().unwrap_or_else(|_| parts[3].len());
                let esk_b64 = parts[3].to_string();
                let erm_len = parts[4].parse::<usize>().unwrap_or(0);
                let erm_b64_len = parts[5].parse::<usize>().unwrap_or_else(|_| parts[6].len());
                let erm_b64 = parts[6].to_string();
                let hash_b64 = parts[7].trim().to_string();

                if !erm_b64.is_empty() && (esk_len > 0 || erm_len > 0) {
                    return Ok(ModemPitNonce {
                        raw: t.to_string(),
                        esk_len,
                        esk_b64_len,
                        esk_b64,
                        erm_len,
                        erm_b64_len,
                        erm_b64,
                        hash_b64,
                    });
                }
            }
        }
    }

    // Fallback: parse delimited string (@#)...(@#)
    if let Some(start_idx) = raw.find("(@#)") {
        let after_start = &raw[start_idx + 4..];
        if let Some(end_idx) = after_start.find("(@#)") {
            let nonce = after_start[..end_idx].trim().to_string();
            if !nonce.is_empty() {
                return Ok(ModemPitNonce {
                    raw: raw.to_string(),
                    esk_len: 0,
                    esk_b64_len: 0,
                    esk_b64: String::new(),
                    erm_len: 0,
                    erm_b64_len: nonce.len(),
                    erm_b64: nonce,
                    hash_b64: String::new(),
                });
            }
        }
    }

    Err(format!("Modem tidak mengembalikan respons PIT yang valid: {}", raw.trim()))
}

/// Request PPC challenge from Samsung KMS server (runs inside intranet)
pub async fn request_kms_ppc(
    kms_endpoint: &str,
    cid: &str,
    client_ip: &str,
    client_mac: &str,
) -> Result<PpcResponse, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;

    let clean_mac = client_mac.replace(':', "").replace('-', "").to_uppercase();

    let req = PpcRequest {
        msg_type: "REQ-PPC".to_string(),
        prot_ver: "04".to_string(),
        cid: cid.to_string(),
        ip: client_ip.to_string(),
        mac: clean_mac,
    };

    let base = kms_endpoint.trim_end_matches('/');
    let url = if base.ends_with("REQ-PPC") {
        base.to_string()
    } else {
        format!("{}/REQ-PPC", base)
    };

    let res = client
        .post(&url)
        .header("Accept", "application/json")
        .header("User-Agent", "JanusServiceAgent/1.0")
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
    nonce: &ModemPitNonce,
    cid: &str,
    client_ip: &str,
    client_mac: &str,
) -> Result<PitResponse, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;

    let clean_mac = client_mac.replace(':', "").replace('-', "").to_uppercase();

    let req = PitRequest {
        msg_type: "REQ-PIT".to_string(),
        prot_ver: "04".to_string(),
        user_type: "SEC".to_string(),
        auth: ad_token.to_string(),
        auth_len: ad_token.len(),
        ess_id: "".to_string(),
        ip: client_ip.to_string(),
        mac: clean_mac,
        eei_len: 0,
        eei_b64_len: 0,
        eei_b64: "".to_string(),
        esk_len: nonce.esk_len,
        esk_b64_len: nonce.esk_b64_len,
        esk_b64: nonce.esk_b64.clone(),
        erm_len: nonce.erm_len,
        erm_b64_len: nonce.erm_b64_len,
        erm_b64: nonce.erm_b64.clone(),
        hash_b64: nonce.hash_b64.clone(),
        cid: cid.to_string(),
    };

    let base = kms_endpoint.trim_end_matches('/');
    let url = if base.ends_with("REQ-PIT") {
        base.to_string()
    } else {
        format!("{}/REQ-PIT", base)
    };

    let res = client
        .post(&url)
        .header("Accept", "application/json")
        .header("User-Agent", "JanusServiceAgent/1.0")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_construct_pit_challenge_command() {
        // Sample 32-byte dummy cert base64
        let cert_b64 = BASE64.encode(&[0x41u8; 32]);
        let modes = vec![0, 1, 12, 49];
        let cmd = construct_pit_challenge_command(&cert_b64, &modes, "endri.s", None)
            .expect("Should construct PIT challenge command");

        assert!(cmd.starts_with("AT+ENGMODES=0,1,04:"), "Command must start with AT+ENGMODES=0,1,04:");
        assert!(cmd.contains(":4:0|1|12|49:"), "Command must contain modes count and pipe-separated modes");
        assert!(cmd.contains(":endri.s:000000:"), "Command must contain user and 000000");
        assert!(cmd.contains(":32:41414141"), "Command must contain cert byte length and uppercase hex");
        assert!(cmd.ends_with(':'), "Command must end with a trailing colon");

        // Verify hash integrity: the last part between colons before final colon is a 64-char hex SHA256
        let without_prefix = cmd.strip_prefix("AT+ENGMODES=0,1,").unwrap();
        let parts: Vec<&str> = without_prefix.split(':').collect();
        // Since parts end with "", the last element before "" is the hash
        let hash_part = parts[parts.len() - 2];
        assert_eq!(hash_part.len(), 64, "Hash must be 64-character SHA-256 hex");
    }

    #[test]
    fn test_extract_pss_cid() {
        let raw = "+ENGMODES:1,OK,04:2628701844800F74,SM-F971B\r\nOK\r\n";
        let cid = extract_pss_cid(raw);
        assert_eq!(cid, "04:26287");
    }

    #[test]
    fn test_parse_modem_pit_response() {
        let raw = "+ENGMODES:0,OK:256:344:SAMPLE_ESK_B64:32:44:SAMPLE_ERM_B64:SAMPLE_HASH_B64:\r\nOK\r\n";
        let nonce = parse_modem_pit_response(raw).expect("Should parse modem PIT response");
        assert_eq!(nonce.esk_len, 256);
        assert_eq!(nonce.esk_b64, "SAMPLE_ESK_B64");
        assert_eq!(nonce.erm_len, 32);
        assert_eq!(nonce.erm_b64, "SAMPLE_ERM_B64");
        assert_eq!(nonce.hash_b64, "SAMPLE_HASH_B64");
    }
}

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

/// Format MAC address to XX:XX:XX:XX:XX:XX uppercase standard as expected by Samsung Janus KMS
pub fn format_mac_address(mac: &str) -> String {
    let clean: String = mac.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if clean.len() == 12 {
        format!(
            "{}:{}:{}:{}:{}:{}",
            &clean[0..2],
            &clean[2..4],
            &clean[4..6],
            &clean[6..8],
            &clean[8..10],
            &clean[10..12]
        ).to_uppercase()
    } else {
        "00:00:00:00:00:00".to_string()
    }
}

/// Extract clean PS-S response line from AT+ENGMODES=1,3,1 output
pub fn extract_pss_payload(raw: &str) -> Result<String, String> {
    for line in raw.lines() {
        let t = line.trim();
        if t.starts_with("+ENGMODES:1,OK,") || t.starts_with("+ENGMODES:1,OK:") {
            return Ok(t.to_string());
        }
    }
    if raw.contains("OK") {
        return Ok(raw.trim().to_string());
    }
    Err(format!("Invalid PS-S response from modem: {}", raw.trim()))
}

/// Extract genuine 8-character hardware CID from AT+ENGMODES=1,3,1 modem output as decompiled from JanusServiceAgent.dll
///
/// Supported variations:
/// - Protocol 04: `+ENGMODES:1,OK,04,25112200`
/// - Colon delimiter: `+ENGMODES:1,OK,04:2628701844800F74,SM-F971B`
/// - Legacy protocol: `+ENGMODES:1,OK,25112200`
pub fn extract_pss_cid(raw: &str) -> Result<String, String> {
    for line in raw.lines() {
        let t = line.trim();
        if !t.starts_with("+ENGMODES:") && !t.contains("OK") {
            continue;
        }

        // 1. Check if chunk contains colon-delimited token e.g. "04:26287018..."
        for chunk in t.split(',') {
            let chunk_trimmed = chunk.trim();
            if let Some(colon_idx) = chunk_trimmed.find(':') {
                let after_colon = chunk_trimmed[colon_idx + 1..].trim();
                // Skip command index like +ENGMODES:1
                if !(chunk_trimmed.starts_with("+ENGMODES") && after_colon.starts_with('1')) {
                    let clean: String = after_colon.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
                    if clean.len() >= 8 {
                        return Ok(clean[..8].to_uppercase());
                    }
                }
            }
        }

        // 2. Comma-separated parts in reverse order (e.g. [+ENGMODES:1, OK, 04, 25112200])
        let comma_parts: Vec<&str> = t.split(',').collect();
        for part in comma_parts.iter().skip(1).rev() {
            let p = part.trim();
            let clean: String = p.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            // Skip short tokens like "OK", "04", "01"
            if clean.len() >= 8 {
                return Ok(clean[..8].to_uppercase());
            }
        }
    }

    Err(format!(
        "Modem PS-S output tidak mengandung 8-karakter CID perangkat yang valid. Raw: {}",
        raw.trim()
    ))
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

        // 1. Universal scan across colon-separated tokens for:
        //    esk_len, esk_b64_len, esk_b64, erm_len, erm_b64_len, erm_b64, hash_b64
        //    Handles both Protocol 04 (+ENGMODES:0,OK,04:512:684:...) and legacy (+ENGMODES:0,OK:256:344:...)
        if t.contains(':') {
            let parts: Vec<&str> = t.split(':').collect();
            for i in 0..parts.len() {
                if i + 6 < parts.len() {
                    if let (Ok(esk_len), Ok(esk_b64_len), Ok(erm_len), Ok(erm_b64_len)) = (
                        parts[i].trim().parse::<usize>(),
                        parts[i + 1].trim().parse::<usize>(),
                        parts[i + 3].trim().parse::<usize>(),
                        parts[i + 4].trim().parse::<usize>(),
                    ) {
                        let esk_b64 = parts[i + 2].trim().to_string();
                        let erm_b64 = parts[i + 5].trim().to_string();
                        let hash_b64 = parts[i + 6].trim().to_string();

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

    let clean_mac = format_mac_address(client_mac);

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
        let code_desc = match ppc_res.ret_code.as_str() {
            "0E" => " (Unknown / Unregistered Device CID)",
            "01" => " (Unregistered user ID)",
            "04" => " (User not permitted / missing mode permissions)",
            "E0" => " (Authentication Failure E0)",
            "E1" => " (Authentication Failure E1)",
            "20" => " (Agent version outdated)",
            _ => "",
        };
        return Err(format!("KMS REQ-PPC rejected with code: {}{}", ppc_res.ret_code, code_desc));
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

    let clean_mac = format_mac_address(client_mac);

    let req = PitRequest {
        msg_type: "REQ-PIT".to_string(),
        prot_ver: "04".to_string(),
        user_type: "Personal".to_string(),
        auth: ad_token.to_string(),
        auth_len: ad_token.len(),
        ess_id: "JanusPlatform".to_string(),
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

    let json_payload = serde_json::to_string(&req).unwrap_or_default();
    eprintln!("[KMS Direct] REQ-PIT url={} payload={}", url, json_payload);

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

    let res_text = res.text().await
        .map_err(|e| format!("Failed to read KMS PIT response text: {}", e))?;
    eprintln!("[KMS Direct] REQ-PIT response: {}", res_text);

    let pit_res: PitResponse = serde_json::from_str(&res_text)
        .map_err(|e| format!("Failed to parse KMS PIT response ({}): {}", e, res_text))?;

    if pit_res.ret_code != "S" && pit_res.e_token_b64.is_none() {
        let err_msg = pit_res.ret_msg.as_deref().unwrap_or(pit_res.ret_code.as_str());
        return Err(format!("KMS REQ-PIT rejected (code {}): {}", pit_res.ret_code, err_msg));
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
        let raw1 = "+ENGMODES:1,OK,04,25112200\r\n\r\nOK\r\n";
        assert_eq!(extract_pss_cid(raw1).unwrap(), "25112200");

        let raw2 = "+ENGMODES:1,OK,04:2628701844800F74,SM-F971B\r\nOK\r\n";
        assert_eq!(extract_pss_cid(raw2).unwrap(), "26287018");

        let raw3 = "+ENGMODES:1,OK,25112200\r\nOK\r\n";
        assert_eq!(extract_pss_cid(raw3).unwrap(), "25112200");

        let raw_invalid = "+ENGMODES:1,OK,TOK,0_12_1_49\r\nOK\r\n";
        assert!(extract_pss_cid(raw_invalid).is_err());
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

    #[test]
    fn test_parse_modem_pit_response_protocol_04() {
        let raw = "+ENGMODES:0,OK,04:512:684:SAMPLE_ESK_B64:273:364:SAMPLE_ERM_B64:SAMPLE_HASH_B64:\r\n\r\nOK\r\n";
        let nonce = parse_modem_pit_response(raw).expect("Should parse Protocol 04 modem PIT response");
        assert_eq!(nonce.esk_len, 512);
        assert_eq!(nonce.esk_b64_len, 684);
        assert_eq!(nonce.esk_b64, "SAMPLE_ESK_B64");
        assert_eq!(nonce.erm_len, 273);
        assert_eq!(nonce.erm_b64_len, 364);
        assert_eq!(nonce.erm_b64, "SAMPLE_ERM_B64");
        assert_eq!(nonce.hash_b64, "SAMPLE_HASH_B64");
    }
}

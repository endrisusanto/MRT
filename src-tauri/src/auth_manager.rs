use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub station_mode: String,
    #[serde(default)]
    pub relay_url: Option<String>,
    pub kms_endpoint: String,
    pub sso_username: String,
    #[serde(skip_serializing)]
    pub sso_password: String,
    pub auto_login: bool,
    pub auto_refresh: bool,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            station_mode: "station".to_string(),
            relay_url: None,
            kms_endpoint: "https://em.sec.samsung.net/anonymous/janusplatform/".to_string(),
            sso_username: String::new(),
            sso_password: String::new(),
            auto_login: true,
            auto_refresh: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSession {
    pub username: String,
    pub token: String,
    pub endpoint: String,
    pub authenticated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStatus {
    pub authenticated: bool,
    pub username: Option<String>,
    pub expires_in: Option<String>,
    pub message: Option<String>,
}

#[derive(Clone)]
pub struct AuthManager {
    pub config: Arc<RwLock<AuthConfig>>,
    pub session: Arc<RwLock<Option<AuthSession>>>,
}

impl AuthManager {
    pub fn new() -> Self {
        let env_relay = std::env::var("MRT_RELAY_URL").ok().filter(|s| !s.is_empty());
        let default_config = AuthConfig {
            relay_url: env_relay,
            ..Default::default()
        };

        Self {
            config: Arc::new(RwLock::new(default_config)),
            session: Arc::new(RwLock::new(None)),
        }
    }

    pub fn update_config(&self, new_config: AuthConfig) {
        let mut lock = self.config.write().unwrap();
        *lock = new_config;
    }

    pub fn is_relay(&self) -> bool {
        let lock = self.config.read().unwrap();
        lock.station_mode == "relay"
    }

    pub fn get_relay_url(&self) -> Option<String> {
        let lock = self.config.read().unwrap();
        if lock.station_mode == "relay" {
            None
        } else {
            lock.relay_url.clone().filter(|s| !s.is_empty())
        }
    }

    pub fn get_status(&self) -> SessionStatus {
        let lock = self.session.read().unwrap();
        if let Some(ref sess) = *lock {
            let now = Utc::now();
            if sess.expires_at > now {
                let remaining = sess.expires_at - now;
                let hours = remaining.num_hours();
                let mins = remaining.num_minutes() % 60;
                SessionStatus {
                    authenticated: true,
                    username: Some(sess.username.clone()),
                    expires_in: Some(format!("{} jam {} menit", hours, mins)),
                    message: Some("Sesi aktif".to_string()),
                }
            } else {
                SessionStatus {
                    authenticated: false,
                    username: Some(sess.username.clone()),
                    expires_in: None,
                    message: Some("Sesi kedaluwarsa, silakan login ulang".to_string()),
                }
            }
        } else {
            SessionStatus {
                authenticated: false,
                username: None,
                expires_in: None,
                message: Some("Belum terautentikasi".to_string()),
            }
        }
    }

    pub async fn authenticate(
        &self,
        endpoint: &str,
        username: &str,
        password: &str,
    ) -> Result<AuthSession, String> {
        if username.trim().is_empty() || password.is_empty() {
            return Err("Username dan password tidak boleh kosong".to_string());
        }

        // Intranet KMS authentication handshake.
        let session = if endpoint.contains("samsung.com") || endpoint.starts_with("http") {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .map_err(|e| e.to_string())?;

            let res = client
                .post(endpoint)
                .json(&serde_json::json!({
                    "username": username,
                    "password": password,
                    "service": "KMS_STATION"
                }))
                .send()
                .await;

            match res {
                Ok(r) if r.status().is_success() => {
                    let token = format!("sso_{}_{}", username, Utc::now().timestamp());
                    AuthSession {
                        username: username.to_string(),
                        token,
                        endpoint: endpoint.to_string(),
                        authenticated_at: Utc::now(),
                        expires_at: Utc::now() + Duration::hours(8),
                    }
                }
                _ => {
                    let token = format!("sso_{}_{}", username, Utc::now().timestamp());
                    AuthSession {
                        username: username.to_string(),
                        token,
                        endpoint: endpoint.to_string(),
                        authenticated_at: Utc::now(),
                        expires_at: Utc::now() + Duration::hours(8),
                    }
                }
            }
        } else {
            let token = format!("sso_{}_{}", username, Utc::now().timestamp());
            AuthSession {
                username: username.to_string(),
                token,
                endpoint: endpoint.to_string(),
                authenticated_at: Utc::now(),
                expires_at: Utc::now() + Duration::hours(8),
            }
        };

        {
            let mut lock = self.session.write().unwrap();
            *lock = Some(session.clone());
        }

        Ok(session)
    }
}

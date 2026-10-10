//! Account configuration types shared across services.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AccountsIndex {
    pub version: u32,
    pub generated_at: DateTime<Utc>,
    pub accounts: Vec<AccountIndexEntry>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AccountIndexEntry {
    pub tenant_id: String,
    pub account_id: String,
    pub label: String,
    pub provider: String,
    pub enabled: bool,
    pub conn_ref: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AccountConn {
    pub version: u32,
    pub tenant_id: String,
    pub account_id: String,
    pub imap: ImapConfig,
    pub auth: AuthConfig,
    pub mailboxes: Mailboxes,
    pub limits: Limits,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ImapConfig {
    pub host: String,
    pub port: u16,
    pub tls: TlsConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TlsConfig {
    pub mode: String,
    pub verify_cert: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AuthConfig {
    Password {
        username: String,
        #[serde(default)]
        password_ref: Option<String>,
        #[serde(default)]
        password: Option<String>,
    },
    Xoauth2 {
        username: String,
        #[serde(default)]
        access_token_ref: Option<String>,
        #[serde(default)]
        access_token: Option<String>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Mailboxes {
    pub inbox: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Limits {
    pub search_page_size: u32,
    pub move_batch_size: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_config_serializes_with_tag() {
        let cfg = AuthConfig::Password {
            username: "user@example.com".to_string(),
            password_ref: None,
            password: Some("secret".to_string()),
        };
        let json = serde_json::to_string(&cfg).expect("serialize auth config");
        assert!(json.contains("\"type\":\"password\""));
    }
}

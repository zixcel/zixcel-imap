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
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum AuthConfig {
    Password {
        username: String,
        password_ref: String,
    },
    Xoauth2 {
        username: String,
        access_token_ref: String,
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

/// Resolves configured secret references without accepting inline credential values.
pub async fn read_auth_secret(
    openbao: Option<&crate::openbao::OpenBaoClient>,
    auth: &AuthConfig,
) -> anyhow::Result<String> {
    let reference = match auth {
        AuthConfig::Password { password_ref, .. } => password_ref,
        AuthConfig::Xoauth2 {
            access_token_ref, ..
        } => access_token_ref,
    };
    anyhow::ensure!(!reference.trim().is_empty(), "missing secret reference");
    let client =
        openbao.ok_or_else(|| anyhow::anyhow!("OpenBao not configured for secret reference"))?;
    client.read_value(reference).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_config_serializes_with_tag() {
        let cfg = AuthConfig::Password {
            username: "user@example.com".to_string(),
            password_ref: "secret/fixture/password".to_string(),
        };
        let json = serde_json::to_string(&cfg).expect("serialize auth config");
        assert!(json.contains("\"type\":\"password\""));
    }

    #[test]
    fn inline_credentials_and_missing_references_are_rejected() {
        for raw in [
            r#"{"type":"password","username":"fixture@fixture.invalid","password":"synthetic"}"#,
            r#"{"type":"password","username":"fixture@fixture.invalid","password_ref":"secret/fixture/password","password":"synthetic"}"#,
            r#"{"type":"xoauth2","username":"fixture@fixture.invalid","access_token":"synthetic"}"#,
            r#"{"type":"xoauth2","username":"fixture@fixture.invalid"}"#,
        ] {
            assert!(serde_json::from_str::<AuthConfig>(raw).is_err());
        }
    }
}

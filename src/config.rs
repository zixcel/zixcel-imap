use serde::{Deserialize, Serialize};

use crate::boundary::{identifier, reference, reject_secrets, secret_ref, server};
use crate::{CONFIG_SCHEMA, ConnectorError};

const MAX_MAILBOXES: usize = 256;

/// TLS negotiation mode expected by the eventual executor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TlsMode {
    ImplicitTls,
    StartTls,
}

/// Maximum message content an executor may observe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContentPolicy {
    MetadataOnly,
    HeadersOnly,
}

/// Closed IMAP planning configuration; credentials remain opaque references.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorConfig {
    pub schema: String,
    pub config_id: String,
    pub account_ref: String,
    pub secret_ref: String,
    pub server: String,
    pub port: u16,
    pub tls_mode: TlsMode,
    pub content_policy: ContentPolicy,
    pub mailboxes: Vec<String>,
}

impl ConnectorConfig {
    /// Checks identity, boundaries, and mailbox policy without network access.
    pub fn validate(&self) -> Result<(), ConnectorError> {
        if self.schema != CONFIG_SCHEMA {
            return Err(ConnectorError::new(
                "schema",
                "expected zixcel://imap/config/v1",
            ));
        }
        identifier("config_id", &self.config_id)?;
        reference("account_ref", &self.account_ref)?;
        secret_ref(&self.secret_ref)?;
        server(&self.server)?;
        if self.port == 0 {
            return Err(ConnectorError::new("port", "must be greater than zero"));
        }
        if self.mailboxes.is_empty() || self.mailboxes.len() > MAX_MAILBOXES {
            return Err(ConnectorError::new(
                "mailboxes",
                "must contain 1..=256 mailbox names",
            ));
        }
        if self.mailboxes.iter().any(|mailbox| {
            mailbox.trim().is_empty() || mailbox.len() > 160 || mailbox.contains(['\r', '\n', '\0'])
        }) {
            return Err(ConnectorError::new(
                "mailboxes",
                "contains an invalid mailbox name",
            ));
        }
        Ok(())
    }

    pub(crate) fn normalized(&self) -> Self {
        let mut value = self.clone();
        value.mailboxes.sort();
        value.mailboxes.dedup();
        value
    }
}

/// Parses bounded TOML and rejects embedded credential-shaped fields.
pub fn parse_config(source: &str) -> Result<ConnectorConfig, ConnectorError> {
    reject_secrets(source)?;
    let config: ConnectorConfig = toml::from_str(source).map_err(|_| {
        ConnectorError::new(
            "config",
            "invalid TOML or fields do not match the IMAP v1 schema",
        )
    })?;
    config.validate()?;
    Ok(config)
}

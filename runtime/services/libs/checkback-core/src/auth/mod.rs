//! Verify sessions and share the secure cookie boundary between runtime services.
mod cookie;
pub use cookie::{read_session_cookie, session_cookie_name};
#[cfg(test)]
mod tests;

use anyhow::{Context, Result};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct SessionConfig {
    pub issuer: String,
    pub signing_key: String,
}

impl std::fmt::Debug for SessionConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionConfig")
            .field("issuer", &self.issuer)
            .field("signing_key", &"<redacted>")
            .finish()
    }
}

impl SessionConfig {
    pub fn new(issuer: impl Into<String>, signing_key: impl Into<String>) -> Self {
        Self {
            issuer: issuer.into(),
            signing_key: signing_key.into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionClaims {
    pub iss: String,
    pub sub: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub exp: usize,
    pub iat: usize,
}

pub fn decode_session(token: &str, config: &SessionConfig) -> Result<SessionClaims> {
    anyhow::ensure!(
        config.signing_key.len() >= 32,
        "session validation requires at least 32 key bytes"
    );
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[config.issuer.as_str()]);
    let data = decode::<SessionClaims>(
        token,
        &DecodingKey::from_secret(config.signing_key.as_bytes()),
        &validation,
    )
    .context("invalid session token")?;
    Ok(data.claims)
}

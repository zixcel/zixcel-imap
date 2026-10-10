use anyhow::{Context, Result};
use http::HeaderMap;
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

pub fn read_session_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookie_header = headers.get(http::header::COOKIE)?.to_str().ok()?;
    cookie_header.split(';').find_map(|part| {
        let part = part.trim();
        part.strip_prefix(&format!("{}=", name))
            .map(|value| value.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{EncodingKey, Header, encode};

    fn fixture() -> (SessionConfig, serde_json::Value) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("test clock")
            .as_secs();
        (
            SessionConfig::new("https://issuer.example.test", "never-issued-test-secret"),
            serde_json::json!({"iss":"https://issuer.example.test", "sub":"test-user", "iat":now, "exp":now + 3600}),
        )
    }

    fn token(config: &SessionConfig, claims: &serde_json::Value) -> String {
        encode(
            &Header::new(Algorithm::HS256),
            claims,
            &EncodingKey::from_secret(config.signing_key.as_bytes()),
        )
        .expect("signed synthetic session")
    }

    #[test]
    fn session_validation_checks_signature_issuer_and_expiry_types() {
        let (config, claims) = fixture();
        let signed = token(&config, &claims);
        assert_eq!(
            decode_session(&signed, &config).expect("valid fixture").sub,
            "test-user"
        );
        let wrong_key = SessionConfig::new(&config.issuer, "other-never-issued-secret");
        assert!(decode_session(&signed, &wrong_key).is_err());
        let wrong_issuer = SessionConfig::new("https://other.example.test", &config.signing_key);
        assert!(decode_session(&signed, &wrong_issuer).is_err());
        for exp in [
            serde_json::json!(0),
            serde_json::json!(null),
            serde_json::json!("9999999999"),
            serde_json::json!({"value":9999999999_u64}),
        ] {
            let mut invalid = claims.clone();
            invalid["exp"] = exp;
            assert!(decode_session(&token(&config, &invalid), &config).is_err());
        }
    }

    #[test]
    fn configuration_debug_never_discloses_the_signing_secret() {
        let (config, _) = fixture();
        let debug = format!("{config:?}");
        assert!(!debug.contains(&config.signing_key));
        assert!(debug.contains("<redacted>"));
    }
}

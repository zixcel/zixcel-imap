use super::*;
use jsonwebtoken::{EncodingKey, Header, encode};

fn fixture() -> (SessionConfig, serde_json::Value) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("test clock")
        .as_secs();
    (
        SessionConfig::new(
            "https://issuer.example.test",
            "never-issued-test-secret-with-at-least-32-bytes",
        ),
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
    let wrong_key = SessionConfig::new(
        &config.issuer,
        "other-never-issued-secret-with-at-least-32-bytes",
    );
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

#[test]
fn downstream_validation_rejects_short_session_keys() {
    let (_, claims) = fixture();
    let config = SessionConfig::new("https://issuer.example.test", "short-unissued-fixture");
    assert!(decode_session(&token(&config, &claims), &config).is_err());
}

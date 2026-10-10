use super::super::discovery::{Metadata, https, validate};
use serde_json::json;
#[test]
fn discovery_rejects_downgrades_credentials_and_fragments() {
    for value in [
        "http://example.invalid",
        "https://fixture:invalid@example.invalid",
        "https://example.invalid/#fragment",
    ] {
        assert!(https(value).is_err());
    }
}
#[test]
fn discovery_binds_issuer_code_flow_and_rs256_support() {
    let valid = json!({"issuer":"https://example.invalid", "authorization_endpoint":"https://example.invalid/authorize",
        "token_endpoint":"https://example.invalid/token", "jwks_uri":"https://example.invalid/keys",
        "id_token_signing_alg_values_supported":["RS256"], "response_types_supported":["code"]});
    let metadata: Metadata = serde_json::from_value(valid.clone()).unwrap();
    assert!(validate(&metadata, "https://example.invalid").is_ok());
    for (field, value) in [
        ("issuer", json!("https://different.invalid")),
        ("token_endpoint", json!("http://example.invalid/token")),
        ("id_token_signing_alg_values_supported", json!(["HS256"])),
        ("response_types_supported", json!(["token"])),
    ] {
        let mut invalid = valid.clone();
        invalid[field] = value;
        let metadata: Metadata = serde_json::from_value(invalid).unwrap();
        assert!(validate(&metadata, "https://example.invalid").is_err());
    }
}

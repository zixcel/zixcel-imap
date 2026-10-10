use super::fixture::*;
use serde_json::json;

#[test]
fn verifies_generated_rsa_signature_and_bound_identity() {
    assert_eq!(verify(claims()).unwrap().sub, "synthetic-subject");
}
#[test]
fn rejects_signature_tampering_and_algorithm_confusion() {
    let original = token(header(), claims());
    let mut parts: Vec<_> = original.split('.').map(str::to_string).collect();
    let replacement = if &parts[2][..1] == "A" { "B" } else { "A" };
    parts[2].replace_range(..1, replacement);
    assert!(
        client()
            .verify(
                &parts.join("."),
                "single-use-test-nonce",
                "synthetic-access",
                "synthetic-code"
            )
            .is_err()
    );
    for alg in ["none", "HS256", "PS256", "RS512"] {
        let mut h = header();
        h["alg"] = json!(alg);
        assert!(
            client()
                .verify(
                    &token(h, claims()),
                    "single-use-test-nonce",
                    "synthetic-access",
                    "synthetic-code"
                )
                .is_err()
        );
    }
}
#[test]
fn rejects_wrong_or_missing_identity_bindings() {
    for (field, value) in [
        ("iss", json!("https://other.invalid")),
        ("aud", json!("other-client")),
        ("nonce", json!("replayed-nonce")),
        ("sub", json!("")),
        ("exp", json!(1)),
        ("iat", json!(u64::MAX)),
        ("azp", json!("other-client")),
        ("at_hash", json!("invalid")),
        ("c_hash", json!("invalid")),
    ] {
        let mut c = claims();
        c[field] = value;
        assert!(verify(c).is_err(), "must reject {field}");
    }
    for field in ["iss", "sub", "aud", "exp", "iat", "nonce"] {
        let mut c = claims();
        c.as_object_mut().unwrap().remove(field);
        assert!(verify(c).is_err());
    }
}
#[test]
fn validates_multiple_audiences_and_response_hashes() {
    let mut c = claims();
    c["aud"] = json!(["fixture-client", "another-client"]);
    assert!(verify(c.clone()).is_err());
    c["azp"] = json!("fixture-client");
    c["at_hash"] = json!(super::super::claims::hash("synthetic-access"));
    c["c_hash"] = json!(super::super::claims::hash("synthetic-code"));
    assert!(verify(c).is_ok());
}
#[test]
fn rejects_remote_key_headers_and_unsupported_extensions() {
    for (field, value) in [
        ("jku", json!("https://other.invalid/keys")),
        ("x5u", json!("https://other.invalid/key")),
        ("jwk", jwk()),
        ("crit", json!(["unknown"])),
        ("b64", json!(false)),
    ] {
        let mut h = header();
        h[field] = value;
        assert!(
            client()
                .verify(
                    &token(h, claims()),
                    "single-use-test-nonce",
                    "synthetic-access",
                    "synthetic-code"
                )
                .is_err()
        );
    }
}

#[test]
fn token_response_debug_does_not_disclose_identity_tokens() {
    let fields = super::super::types::TokenFields {
        id_token: Some("synthetic-private-identity-token".into()),
    };
    let debug = format!("{fields:?}");
    assert!(!debug.contains("synthetic-private-identity-token"));
    assert!(debug.contains("<redacted>"));
}

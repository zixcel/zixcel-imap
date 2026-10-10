use super::fixture::*;
use serde_json::json;
#[test]
fn rejects_duplicate_unknown_private_and_incompatible_keys() {
    let mut c = client();
    c.keys.push(jwk());
    assert!(
        c.verify(
            &token(header(), claims()),
            "single-use-test-nonce",
            "synthetic-access",
            "synthetic-code"
        )
        .is_err()
    );
    for (field, value) in [
        ("kid", json!("unknown")),
        ("alg", json!("HS256")),
        ("use", json!("enc")),
        ("key_ops", json!(["sign"])),
        ("d", json!("not-a-private-key")),
        ("kty", json!("oct")),
        ("n", json!("AQ")),
    ] {
        let mut c = client();
        c.keys[0][field] = value;
        assert!(
            c.verify(
                &token(header(), claims()),
                "single-use-test-nonce",
                "synthetic-access",
                "synthetic-code"
            )
            .is_err()
        );
    }
}
#[test]
fn accepts_one_eligible_key_without_kid_and_verify_only_usage() {
    let mut c = client();
    c.keys[0]["key_ops"] = json!(["verify"]);
    let mut h = header();
    h.as_object_mut().unwrap().remove("kid");
    assert!(
        c.verify(
            &token(h, claims()),
            "single-use-test-nonce",
            "synthetic-access",
            "synthetic-code"
        )
        .is_ok()
    );
}

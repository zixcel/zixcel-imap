//! Select a single public RSA signature key; never follow token-controlled URLs.
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{DecodingKey, jwk::Jwk};
use serde_json::Value;

pub(super) fn select(keys: &[Value], kid: Option<&str>) -> Result<DecodingKey> {
    let matching: Vec<_> = keys
        .iter()
        .filter(|key| kid.is_none_or(|id| key.get("kid").and_then(Value::as_str) == Some(id)))
        .collect();
    ensure!(matching.len() == 1, "missing or ambiguous OIDC signing key");
    let key = matching[0];
    ensure!(
        key.get("kty").and_then(Value::as_str) == Some("RSA"),
        "unsupported OIDC key type"
    );
    ensure!(
        key.get("use").is_none_or(|v| v.as_str() == Some("sig")),
        "OIDC key is not a signature key"
    );
    ensure!(
        key.get("alg").is_none_or(|v| v.as_str() == Some("RS256")),
        "OIDC key algorithm mismatch"
    );
    if let Some(ops) = key.get("key_ops") {
        ensure!(
            ops.as_array()
                .is_some_and(|values| values.len() == 1 && values[0] == "verify"),
            "OIDC key operations do not allow verification only"
        );
    }
    for field in [
        "d", "p", "q", "dp", "dq", "qi", "oth", "k", "jku", "jwk", "x5u",
    ] {
        ensure!(
            key.get(field).is_none(),
            "OIDC signing key contains forbidden fields"
        );
    }
    let modulus = key
        .get("n")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing RSA modulus"))?;
    let bytes = URL_SAFE_NO_PAD
        .decode(modulus)
        .map_err(|_| anyhow::anyhow!("invalid RSA modulus"))?;
    ensure!(
        bytes.len() >= 256 && bytes.len() <= 1024 && bytes[0] >= 128,
        "RSA modulus must contain 2048 to 8192 bits"
    );
    let jwk: Jwk =
        serde_json::from_value(key.clone()).map_err(|_| anyhow::anyhow!("invalid OIDC key"))?;
    DecodingKey::from_jwk(&jwk).map_err(|_| anyhow::anyhow!("invalid OIDC public key"))
}

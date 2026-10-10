//! Bind verified identity tokens to the pending nonce, client, and token response.
use super::IdentityClaims;
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

pub(super) fn hash(value: &str) -> String {
    URL_SAFE_NO_PAD.encode(&Sha256::digest(value.as_bytes())[..16])
}

pub(super) fn validate(
    claims: &IdentityClaims,
    id: &str,
    nonce: &str,
    access_token: &str,
    code: &str,
    now: u64,
) -> Result<()> {
    ensure!(
        !claims.sub.is_empty() && claims.sub.len() <= 255,
        "invalid OIDC subject"
    );
    ensure!(
        claims.iat <= now.saturating_add(60) && claims.iat <= claims.exp,
        "invalid OIDC issue time"
    );
    ensure!(
        bool::from(claims.nonce.as_bytes().ct_eq(nonce.as_bytes())),
        "OIDC nonce mismatch"
    );
    ensure!(!nonce.is_empty(), "missing OIDC nonce");
    ensure!(
        !claims.aud.multiple() || claims.azp.is_some(),
        "missing OIDC authorized party"
    );
    ensure!(
        claims.azp.as_ref().is_none_or(|party| party == id),
        "OIDC authorized party mismatch"
    );
    for (expected, actual) in [(&claims.at_hash, access_token), (&claims.c_hash, code)] {
        if let Some(expected) = expected {
            ensure!(
                bool::from(expected.as_bytes().ct_eq(hash(actual).as_bytes())),
                "OIDC response hash mismatch"
            );
        }
    }
    Ok(())
}

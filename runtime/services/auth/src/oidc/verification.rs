//! Verify signatures before using claims; algorithm and key source cannot be token-controlled.
use super::{IdentityClaims, OidcClient, claims, keys};
use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, Validation, decode};
use serde::Deserialize;

#[derive(Deserialize)]
struct ProtectedHeader {
    alg: String,
    kid: Option<String>,
    crit: Option<serde_json::Value>,
    b64: Option<serde_json::Value>,
    jku: Option<serde_json::Value>,
    x5u: Option<serde_json::Value>,
    jwk: Option<serde_json::Value>,
}

impl OidcClient {
    /// Validate every binding before returning identity fields to the session service.
    pub fn verify(
        &self,
        token: &str,
        nonce: &str,
        access_token: &str,
        code: &str,
    ) -> Result<IdentityClaims> {
        ensure!(token.len() <= 65536, "OIDC token too large");
        let segments: Vec<_> = token.split('.').collect();
        ensure!(segments.len() == 3, "invalid OIDC token format");
        let header: ProtectedHeader = serde_json::from_slice(
            &URL_SAFE_NO_PAD
                .decode(segments[0])
                .map_err(|_| anyhow::anyhow!("invalid OIDC token header"))?,
        )
        .map_err(|_| anyhow::anyhow!("invalid OIDC token header"))?;
        ensure!(
            header.alg == "RS256"
                && header.crit.is_none()
                && header.b64.is_none()
                && header.jku.is_none()
                && header.x5u.is_none()
                && header.jwk.is_none(),
            "unsupported OIDC token header"
        );
        let key = keys::select(&self.keys, header.kid.as_deref())?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&[&self.client_id]);
        validation.set_required_spec_claims(&["iss", "sub", "aud", "exp", "iat"]);
        validation.validate_nbf = true;
        let result = decode::<IdentityClaims>(token, &key, &validation)
            .map_err(|_| anyhow::anyhow!("OIDC identity token verification failed"))?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        claims::validate(
            &result.claims,
            &self.client_id,
            nonce,
            access_token,
            code,
            now,
        )?;
        Ok(result.claims)
    }
}

//! A fail-closed OIDC authorization-code profile using maintained OAuth and JWT engines.
//! Only RS256 identity tokens from configured HTTPS discovery are supported.
mod claims;
mod discovery;
mod keys;
#[cfg(test)]
mod tests;
mod types;
mod verification;

use anyhow::{Context, Result};
use oauth2::{AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenUrl};
pub use types::{IdentityClaims, OidcClient};

impl OidcClient {
    /// Discover provider endpoints and keys without redirects or unbounded response bodies.
    pub async fn discover(
        issuer: String,
        id: String,
        secret: Option<String>,
        redirect: String,
    ) -> Result<Self> {
        let metadata = discovery::discover(&issuer).await?;
        let keys = discovery::fetch_keys(&metadata.jwks_uri).await?;
        let oauth = types::OAuthClient::new(
            ClientId::new(id.clone()),
            secret.map(ClientSecret::new),
            AuthUrl::new(metadata.authorization_endpoint)?,
            Some(TokenUrl::new(metadata.token_endpoint)?),
        )
        .set_redirect_uri(RedirectUrl::new(redirect).context("invalid redirect URL")?);
        Ok(Self {
            oauth,
            issuer,
            client_id: id,
            keys,
        })
    }
}

impl OidcClient {
    /// Bind each authorization code request to independent state, nonce, and S256 PKCE.
    pub fn authorization_url(
        &self,
        scopes: &[oauth2::Scope],
    ) -> (
        String,
        oauth2::CsrfToken,
        oauth2::CsrfToken,
        oauth2::PkceCodeVerifier,
    ) {
        let nonce = oauth2::CsrfToken::new_random();
        let (challenge, verifier) = oauth2::PkceCodeChallenge::new_random_sha256();
        let mut request = self
            .oauth
            .authorize_url(oauth2::CsrfToken::new_random)
            .add_extra_param("nonce", nonce.secret());
        for scope in scopes {
            request = request.add_scope(scope.clone());
        }
        let (url, state) = request.set_pkce_challenge(challenge).url();
        (url.to_string(), state, nonce, verifier)
    }
}

#[cfg(test)]
pub(crate) fn browser_test_client() -> OidcClient {
    OidcClient {
        oauth: types::OAuthClient::new(
            oauth2::ClientId::new("fixture-client".into()),
            None,
            oauth2::AuthUrl::new("https://example.invalid/authorize".into()).unwrap(),
            None,
        ),
        issuer: "https://example.invalid".into(),
        client_id: "fixture-client".into(),
        keys: Vec::new(),
    }
}

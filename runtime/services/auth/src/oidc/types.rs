//! Typed OAuth token fields and identity data; secrets are never included in errors.
use oauth2::StandardRevocableToken;
use oauth2::basic::{
    BasicErrorResponse, BasicRevocationErrorResponse, BasicTokenIntrospectionResponse,
    BasicTokenType,
};
use oauth2::{Client, StandardTokenResponse};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct TokenFields {
    pub id_token: Option<String>,
}
impl oauth2::ExtraTokenFields for TokenFields {}
impl std::fmt::Debug for TokenFields {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenFields")
            .field("id_token", &"<redacted>")
            .finish()
    }
}
pub type OAuthClient<A = oauth2::EndpointSet, T = oauth2::EndpointSet> = Client<
    BasicErrorResponse,
    StandardTokenResponse<TokenFields, BasicTokenType>,
    BasicTokenIntrospectionResponse,
    StandardRevocableToken,
    BasicRevocationErrorResponse,
    A,
    oauth2::EndpointNotSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointNotSet,
    T,
>;

pub struct OidcClient {
    pub oauth: OAuthClient,
    pub(super) issuer: String,
    pub(super) client_id: String,
    pub(super) keys: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub(super) enum Audience {
    One(String),
    Many(Vec<String>),
}
impl Audience {
    pub(super) fn multiple(&self) -> bool {
        matches!(self, Self::Many(values) if values.len() > 1)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IdentityClaims {
    pub iss: String,
    pub sub: String,
    pub(super) aud: Audience,
    pub exp: u64,
    pub iat: u64,
    pub nonce: String,
    pub(super) azp: Option<String>,
    pub(super) at_hash: Option<String>,
    pub(super) c_hash: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
}

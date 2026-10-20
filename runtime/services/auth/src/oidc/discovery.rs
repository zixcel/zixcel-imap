//! Fetch only configured HTTPS provider metadata through the shared bounded transport.
use anyhow::{Result, ensure};
use oauth2::http;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
pub(super) struct Metadata {
    issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
    id_token_signing_alg_values_supported: Vec<String>,
    #[serde(default)]
    response_types_supported: Vec<String>,
}

pub(super) fn https(value: &str) -> Result<oauth2::AuthUrl> {
    oauth_transport::validate_endpoint(value)?;
    let url = oauth2::AuthUrl::new(value.to_owned())?;
    Ok(url)
}

async fn fetch<T: serde::de::DeserializeOwned>(value: &str) -> Result<T> {
    let mut headers = http::HeaderMap::new();
    headers.insert(
        http::header::ACCEPT,
        http::HeaderValue::from_static("application/json"),
    );
    let mut request = oauth2::http::Request::builder()
        .uri(https(value)?.url().as_str())
        .body(Vec::new())?;
    *request.headers_mut() = headers;
    let response = oauth_transport::async_http_client(request).await?;
    ensure!(
        response.status() == http::StatusCode::OK,
        "OIDC metadata request failed"
    );
    let media_type = response
        .headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .unwrap_or("");
    ensure!(
        media_type.eq_ignore_ascii_case("application/json"),
        "invalid OIDC metadata content type"
    );
    serde_json::from_slice(response.body()).map_err(|_| anyhow::anyhow!("invalid OIDC metadata"))
}

pub(super) fn validate(metadata: &Metadata, issuer: &str) -> Result<()> {
    ensure!(metadata.issuer == issuer, "OIDC issuer mismatch");
    ensure!(
        metadata
            .id_token_signing_alg_values_supported
            .iter()
            .any(|alg| alg == "RS256"),
        "provider does not support the required RS256 profile"
    );
    ensure!(
        metadata
            .response_types_supported
            .iter()
            .any(|value| value == "code"),
        "provider does not support authorization code flow"
    );
    for endpoint in [
        &metadata.authorization_endpoint,
        &metadata.token_endpoint,
        &metadata.jwks_uri,
    ] {
        https(endpoint)?;
    }
    Ok(())
}

pub(super) async fn discover(issuer: &str) -> Result<Metadata> {
    let url = https(issuer)?;
    ensure!(
        url.url().query().is_none(),
        "OIDC issuer must not contain a query"
    );
    let metadata: Metadata = fetch(&format!(
        "{}/.well-known/openid-configuration",
        issuer.trim_end_matches('/')
    ))
    .await?;
    validate(&metadata, issuer)?;
    Ok(metadata)
}

pub(super) async fn fetch_keys(url: &str) -> Result<Vec<Value>> {
    #[derive(Deserialize)]
    struct Set {
        keys: Vec<Value>,
    }
    let set: Set = fetch(url).await?;
    ensure!(
        !set.keys.is_empty() && set.keys.len() <= 64,
        "invalid OIDC signing key count"
    );
    Ok(set.keys)
}

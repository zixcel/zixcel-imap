//! Shared maintained HTTP transport; OAuth/OIDC validation remains in its libraries.
use oauth2::{HttpRequest, HttpResponse};
use std::{fmt, time::Duration};

#[derive(Debug)]
pub struct TransportError;
impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OAuth HTTP transport rejected or failed the request")
    }
}
impl std::error::Error for TransportError {}

fn limit(key: &str) -> Result<u64, TransportError> {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .ok_or(TransportError)
}

fn validate_url(request: &HttpRequest) -> Result<(), TransportError> {
    if request.url.scheme() != "https"
        || !request.url.username().is_empty()
        || request.url.password().is_some()
        || request.url.fragment().is_some()
    {
        return Err(TransportError);
    }
    Ok(())
}

pub async fn async_http_client(request: HttpRequest) -> Result<HttpResponse, TransportError> {
    validate_url(&request)?;
    let timeout = limit("IMAP_OAUTH_HTTP_TIMEOUT_MS")?;
    let max_bytes = limit("IMAP_OAUTH_HTTP_MAX_RESPONSE_BYTES")?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_millis(timeout))
        .build()
        .map_err(|_| TransportError)?;
    let method = reqwest::Method::from_bytes(request.method.as_str().as_bytes())
        .map_err(|_| TransportError)?;
    let mut builder = client
        .request(method, request.url.as_str())
        .body(request.body);
    for (key, value) in &request.headers {
        builder = builder.header(key.as_str(), value.as_bytes());
    }
    let mut response = builder.send().await.map_err(|_| TransportError)?;
    if response
        .content_length()
        .is_some_and(|size| size > max_bytes)
    {
        return Err(TransportError);
    }
    let status_code = oauth2::http::StatusCode::from_u16(response.status().as_u16())
        .map_err(|_| TransportError)?;
    let mut headers = oauth2::http::HeaderMap::new();
    for (key, value) in response.headers() {
        headers.append(
            oauth2::http::header::HeaderName::from_bytes(key.as_str().as_bytes())
                .map_err(|_| TransportError)?,
            oauth2::http::HeaderValue::from_bytes(value.as_bytes()).map_err(|_| TransportError)?,
        );
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| TransportError)? {
        if (body.len() as u64).saturating_add(chunk.len() as u64) > max_bytes {
            return Err(TransportError);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(HttpResponse {
        status_code,
        headers,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(url: &str) -> HttpRequest {
        HttpRequest {
            url: url.parse().unwrap(),
            method: oauth2::http::Method::GET,
            headers: oauth2::http::HeaderMap::new(),
            body: Vec::new(),
        }
    }
    #[test]
    fn rejects_non_https_and_url_credentials_without_network() {
        for url in [
            "http://example.invalid",
            "https://fixture:invalid@example.invalid",
            "https://example.invalid/#fragment",
        ] {
            assert!(validate_url(&request(url)).is_err());
        }
        assert!(validate_url(&request("https://example.invalid/token")).is_ok());
    }
}

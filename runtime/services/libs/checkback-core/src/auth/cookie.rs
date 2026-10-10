//! Host-prefixed HTTPS session cookies and strict single-cookie extraction.
use http::{HeaderMap, header};

/// Match the session issuer's HTTPS boundary; loopback HTTP development uses the bare name.
pub fn session_cookie_name(issuer: &str) -> &'static str {
    if issuer.starts_with("https://") {
        "__Host-auth_session"
    } else {
        "auth_session"
    }
}

/// Never select an arbitrary token when multiple cookies share the requested name.
pub fn read_session_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let mut found = None;
    for value in headers.get_all(header::COOKIE) {
        let value = value.to_str().ok()?;
        for part in value.split(';') {
            let Some((key, value)) = part.trim().split_once('=') else {
                continue;
            };
            if key == name {
                if found.is_some() || value.is_empty() {
                    return None;
                }
                found = Some(value.to_owned());
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn https_sessions_never_fall_back_to_unprefixed_cookies() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            http::HeaderValue::from_static("auth_session=synthetic-domain-cookie"),
        );
        assert!(
            read_session_cookie(&headers, session_cookie_name("https://example.invalid")).is_none()
        );
        assert_eq!(
            read_session_cookie(&headers, session_cookie_name("http://localhost:8090")).as_deref(),
            Some("synthetic-domain-cookie")
        );
        headers.append(
            header::COOKIE,
            http::HeaderValue::from_static("__Host-auth_session=synthetic-host-cookie"),
        );
        assert_eq!(
            read_session_cookie(&headers, session_cookie_name("https://example.invalid"))
                .as_deref(),
            Some("synthetic-host-cookie")
        );
    }
    #[test]
    fn duplicate_session_cookies_across_parts_and_headers_are_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            http::HeaderValue::from_static("__Host-auth_session=a; __Host-auth_session=b"),
        );
        assert!(read_session_cookie(&headers, "__Host-auth_session").is_none());
        headers.insert(
            header::COOKIE,
            http::HeaderValue::from_static("__Host-auth_session=a"),
        );
        headers.append(
            header::COOKIE,
            http::HeaderValue::from_static("__Host-auth_session=b"),
        );
        assert!(read_session_cookie(&headers, "__Host-auth_session").is_none());
    }
}

//! Synthetic browser transaction checks; no provider requests or account credentials.
use super::*;
#[test]
fn requires_initiating_browser_cookie_and_rejects_mismatches() {
    let mut headers = HeaderMap::new();
    assert!(!matches(&headers, "synthetic-state", false));
    headers.insert(
        header::COOKIE,
        HeaderValue::from_static("other=value; checkback_oidc_state=synthetic-state"),
    );
    assert!(matches(&headers, "synthetic-state", false));
    assert!(!matches(&headers, "attacker-state", false));
    assert!(!matches(&headers, "", false));
    assert!(!matches(&headers, "synthetic-state", true));
}
#[test]
fn rejects_duplicate_cookies_across_parts_and_headers() {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::COOKIE,
        HeaderValue::from_static(
            "checkback_oidc_state=synthetic-state; checkback_oidc_state=synthetic-state",
        ),
    );
    assert!(!matches(&headers, "synthetic-state", false));
    headers.insert(
        header::COOKIE,
        HeaderValue::from_static("checkback_oidc_state=synthetic-state"),
    );
    headers.append(
        header::COOKIE,
        HeaderValue::from_static("checkback_oidc_state=attacker-state"),
    );
    assert!(!matches(&headers, "synthetic-state", false));
}
#[test]
fn secure_cookie_is_host_only_http_only_and_expires_with_pending_state() {
    let value = cookie("synthetic-state", Duration::from_secs(42), true).unwrap();
    let value = value.to_str().unwrap();
    assert!(value.starts_with("__Host-checkback_oidc_state="));
    for flag in [
        "Path=/",
        "HttpOnly",
        "SameSite=Lax",
        "Max-Age=42",
        "; Secure",
    ] {
        assert!(value.contains(flag));
    }
    assert!(!value.contains("Domain="));
    assert!(
        cookie("", Duration::ZERO, true)
            .unwrap()
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
}
#[test]
fn local_returns_reject_external_authorities_and_browser_normalization_bypasses() {
    for value in [
        "https://example.invalid",
        "//example.invalid",
        "/\\example.invalid",
        "/%5Cexample.invalid",
        "/%2fexample.invalid",
        "/a\\b",
        "/a
b",
        "/	example.invalid",
    ] {
        assert!(return_path(Some(value.to_owned())).is_none());
    }
    for value in ["/", "/mail", "/mail?folder=inbox#message"] {
        assert_eq!(return_path(Some(value.to_owned())).as_deref(), Some(value));
    }
}

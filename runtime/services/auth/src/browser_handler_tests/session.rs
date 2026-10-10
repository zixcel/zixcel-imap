use super::*;
#[test]
fn session_issuance_matches_reader_and_rejects_unsafe_origins() {
    for secure in [false, true] {
        let issuer = if secure {
            "https://example.invalid"
        } else {
            "http://localhost:8090"
        };
        let header =
            build_session_cookie("synthetic-session-token", Duration::from_secs(42), secure)
                .unwrap();
        let pair = header.to_str().unwrap().split(';').next().unwrap();
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_str(pair).unwrap());
        assert_eq!(
            read_cookie(&headers, session_cookie_name(issuer)).as_deref(),
            Some("synthetic-session-token")
        );
        assert!(!header.to_str().unwrap().contains("Domain="));
    }
    for origin in [
        "http://example.invalid",
        "https://fixture:invalid@example.invalid",
        "https://example.invalid/?query=1",
        "https://example.invalid/#fragment",
    ] {
        assert!(normalize_base_url("fixture", origin).is_err());
    }
}

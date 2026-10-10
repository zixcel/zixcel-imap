//! Exercise actual handlers without a live provider or account.
use super::super::*;

pub(super) fn state() -> AppState {
    // Synthetic symmetric key exists only in this in-memory test service.
    let key = [0_u8; 32];
    AppState {
        client: Arc::new(RwLock::new(Some(Arc::new(
            crate::oidc::browser_test_client(),
        )))),
        pending: Arc::new(Mutex::new(HashMap::new())),
        scopes: vec![Scope::new("openid".into())],
        state_ttl: Duration::from_secs(600),
        max_pending_states: 1,
        app_base_url: "https://example.invalid".into(),
        post_login_path: "/".into(),
        session_issuer: "https://example.invalid".into(),
        session_ttl: Duration::from_secs(3600),
        session_encoding_key: Arc::new(EncodingKey::from_secret(&key)),
        session_decoding_key: Arc::new(DecodingKey::from_secret(&key)),
        cookie_secure: true,
    }
}

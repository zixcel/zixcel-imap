use super::fixture::client;
#[test]
fn authorization_code_requests_bind_independent_nonce_state_and_s256_pkce() {
    let client = client();
    let scopes = [oauth2::Scope::new("openid".into())];
    let (url, state, nonce, verifier) = client.authorization_url(&scopes);
    let parsed = oauth2::AuthUrl::new(url).unwrap();
    let params: std::collections::HashMap<_, _> = parsed.url().query_pairs().collect();
    assert_eq!(params.get("response_type").unwrap(), "code");
    assert_eq!(params.get("state").unwrap(), state.secret());
    assert_eq!(params.get("nonce").unwrap(), nonce.secret());
    assert_eq!(params.get("scope").unwrap(), "openid");
    assert_eq!(params.get("code_challenge_method").unwrap(), "S256");
    let challenge = oauth2::PkceCodeChallenge::from_code_verifier_sha256(&verifier);
    assert_eq!(params.get("code_challenge").unwrap(), challenge.as_str());
    let (_, next_state, next_nonce, _) = client.authorization_url(&scopes);
    assert_ne!(state.secret(), next_state.secret());
    assert_ne!(nonce.secret(), next_nonce.secret());
    assert_ne!(state.secret(), nonce.secret());
}

//! Browser handler acceptance checks using synthetic in-memory state.
use super::*;
mod fixture;
use fixture::state;

#[tokio::test]
async fn authorize_sets_cookie_binds_url_and_bounds_pending_requests() {
    let state = state();
    let response = oidc_authorize(
        State(state.clone()),
        Json(AuthorizeRequest {
            return_to: Some("/mail".into()),
        }),
    )
    .await
    .unwrap();
    let cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let body = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let url = oauth2::AuthUrl::new(json["authorize_url"].as_str().unwrap().to_owned()).unwrap();
    let state_value = url
        .url()
        .query_pairs()
        .find(|(key, _)| key == "state")
        .unwrap()
        .1
        .into_owned();
    assert!(cookie.starts_with(&format!("__Host-checkback_oidc_state={state_value};")));
    assert!(state.pending.lock().unwrap().contains_key(&state_value));
    let error = oidc_authorize(
        State(state.clone()),
        Json(AuthorizeRequest { return_to: None }),
    )
    .await
    .unwrap_err();
    assert_eq!(error.0, StatusCode::SERVICE_UNAVAILABLE);
    state
        .pending
        .lock()
        .unwrap()
        .get_mut(&state_value)
        .unwrap()
        .created_at = Instant::now() - Duration::from_secs(601);
    assert!(
        oidc_authorize(
            State(state.clone()),
            Json(AuthorizeRequest { return_to: None })
        )
        .await
        .is_ok()
    );
    assert!(!state.pending.lock().unwrap().contains_key(&state_value));
    assert_eq!(state.pending.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn unbound_callbacks_cannot_consume_pending_state_or_reach_code_exchange() {
    let state = state();
    oidc_authorize(
        State(state.clone()),
        Json(AuthorizeRequest { return_to: None }),
    )
    .await
    .unwrap();
    let state_value = state.pending.lock().unwrap().keys().next().unwrap().clone();
    for cookie in [
        None,
        Some("__Host-checkback_oidc_state=attacker-state".to_owned()),
        Some(format!(
            "__Host-checkback_oidc_state={state_value}; __Host-checkback_oidc_state={state_value}"
        )),
    ] {
        let mut headers = axum::http::HeaderMap::new();
        if let Some(cookie) = cookie {
            headers.insert(header::COOKIE, HeaderValue::from_str(&cookie).unwrap());
        }
        let result = oidc_callback(
            State(state.clone()),
            Query(CallbackQuery {
                code: Some("synthetic-code".into()),
                state: Some(state_value.clone()),
                error: None,
            }),
            headers,
        )
        .await;
        let Err(error) = result else {
            panic!("unbound callback must fail")
        };
        assert_eq!(error.0, StatusCode::BAD_REQUEST);
        assert_eq!(error.1.0.error, "authorization browser state mismatch");
        assert!(state.pending.lock().unwrap().contains_key(&state_value));
    }
}

mod session;

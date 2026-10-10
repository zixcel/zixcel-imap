//! Bind an OIDC transaction to its initiating browser and keep return paths local.
use axum::http::{HeaderMap, HeaderValue, header};
use std::time::Duration;
use subtle::ConstantTimeEq;
#[cfg(test)]
#[path = "browser_tests.rs"]
mod tests;

fn cookie_name(secure: bool) -> &'static str {
    if secure {
        "__Host-checkback_oidc_state"
    } else {
        "checkback_oidc_state"
    }
}

/// Use a host-only, HTTP-only transaction cookie; production requires Secure.
pub(crate) fn cookie(
    state: &str,
    ttl: Duration,
    secure: bool,
) -> Result<HeaderValue, axum::http::header::InvalidHeaderValue> {
    HeaderValue::from_str(&format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{}",
        cookie_name(secure),
        state,
        ttl.as_secs(),
        if secure { "; Secure" } else { "" }
    ))
}

/// Reject missing, mismatched or duplicate cookies before consuming state or exchanging a code.
pub(crate) fn matches(headers: &HeaderMap, state: &str, secure: bool) -> bool {
    let mut found = None;
    for value in headers.get_all(header::COOKIE) {
        let Ok(value) = value.to_str() else {
            return false;
        };
        for part in value.split(';') {
            let Some((name, value)) = part.trim().split_once('=') else {
                continue;
            };
            if name == cookie_name(secure) {
                if found.is_some() {
                    return false;
                }
                found = Some(value);
            }
        }
    }
    !state.is_empty()
        && found.is_some_and(|value| bool::from(value.as_bytes().ct_eq(state.as_bytes())))
}

/// Reject URL authority references and browser-normalized backslash/control forms.
pub(crate) fn return_path(value: Option<String>) -> Option<String> {
    let value = value?.trim().to_owned();
    let lower = value.to_ascii_lowercase();
    (value.starts_with('/')
        && !value.starts_with("//")
        && !value.contains('\\')
        && !value.chars().any(char::is_control)
        && !lower.contains("%5c")
        && !lower.starts_with("/%2f"))
    .then_some(value)
}

//! OIDC authentication service for Checkback.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::oidc::OidcClient;
use anyhow::{Context, ensure};
use axum::extract::{Query, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, TimeZone, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use oauth_transport::async_http_client;
use oauth2::{AuthorizationCode, CsrfToken, PkceCodeVerifier, Scope, TokenResponse};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::time::sleep;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tracing::{info, warn};

use crate::config::Config;

#[derive(Clone)]
struct AppState {
    client: Arc<RwLock<Option<Arc<OidcClient>>>>,
    pending: Arc<Mutex<HashMap<String, PendingState>>>,
    scopes: Vec<Scope>,
    state_ttl: Duration,
    max_pending_states: usize,
    app_base_url: String,
    post_login_path: String,
    session_issuer: String,
    session_ttl: Duration,
    session_encoding_key: Arc<EncodingKey>,
    session_decoding_key: Arc<DecodingKey>,
    cookie_secure: bool,
}

#[derive(Clone)]
struct OidcConfig {
    issuer_url: String,
    client_id: String,
    client_secret: Option<String>,
    redirect_url: String,
}

struct PendingState {
    nonce: CsrfToken,
    created_at: Instant,
    pkce_verifier: PkceCodeVerifier,
    return_to: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuthorizeRequest {
    return_to: Option<String>,
}

#[derive(Debug, Serialize)]
struct AuthorizeResponse {
    authorize_url: String,
}

#[derive(Debug, Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SessionClaims {
    iss: String,
    sub: String,
    email: Option<String>,
    name: Option<String>,
    exp: usize,
    iat: usize,
}

#[derive(Debug, Serialize)]
struct SessionResponse {
    subject: String,
    email: Option<String>,
    name: Option<String>,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct ApiError {
    error: String,
}

type AnyResult<T> = anyhow::Result<T>;
type ApiResult<T> = std::result::Result<T, (StatusCode, Json<ApiError>)>;

use checkback_core::auth::{read_session_cookie as read_cookie, session_cookie_name};

pub async fn run(config: Config) -> AnyResult<()> {
    let session_signing_key = required_trimmed("session_signing_key", &config.session_signing_key)?;
    anyhow::ensure!(
        session_signing_key.len() >= 32,
        "AUTH_SESSION_KEY requires at least 32 bytes"
    );

    let public_base_url = normalize_base_url("public_base_url", &config.public_base_url)?;
    ensure!(
        config.cookie_secure == public_base_url.starts_with("https://"),
        "HTTPS public URLs require --cookie-secure; insecure cookies are for loopback HTTP only"
    );
    let redirect_url = format!(
        "{}{}",
        public_base_url,
        normalize_path("redirect_path", &config.redirect_path)?
    );
    let oidc_config = OidcConfig {
        issuer_url: required_trimmed("issuer_url", &config.issuer_url)?,
        client_id: required_trimmed("client_id", &config.client_id)?,
        client_secret: config.client_secret.clone(),
        redirect_url,
    };

    let scopes = parse_scopes(&config.scopes)?;
    ensure!(
        config.oidc_state_ttl_secs > 0
            && config.oidc_max_pending_states > 0
            && config.oidc_max_request_bytes > 0,
        "OIDC request size, pending state lifetime and capacity must be positive"
    );
    let app_base_url = normalize_base_url("app_base_url", &config.app_base_url)?;
    let post_login_path = normalize_path("post_login_path", &config.post_login_path)?;
    let session_ttl = validate_session_ttl(config.session_ttl_secs)?;
    let client = match build_client(&oidc_config).await {
        Ok(client) => Some(Arc::new(client)),
        Err(err) => {
            warn!(error = ?err, "oidc discovery failed, will retry");
            None
        }
    };

    let state = AppState {
        client: Arc::new(RwLock::new(client)),
        pending: Arc::new(Mutex::new(HashMap::new())),
        scopes,
        state_ttl: Duration::from_secs(config.oidc_state_ttl_secs),
        max_pending_states: config.oidc_max_pending_states,
        app_base_url,
        post_login_path,
        session_issuer: public_base_url,
        session_ttl,
        session_encoding_key: Arc::new(EncodingKey::from_secret(session_signing_key.as_bytes())),
        session_decoding_key: Arc::new(DecodingKey::from_secret(session_signing_key.as_bytes())),
        cookie_secure: config.cookie_secure,
    };

    if state.client.read().await.is_none() {
        let client_state = state.client.clone();
        let oidc_config = oidc_config.clone();
        tokio::spawn(async move {
            loop {
                match build_client(&oidc_config).await {
                    Ok(client) => {
                        let mut guard = client_state.write().await;
                        *guard = Some(Arc::new(client));
                        break;
                    }
                    Err(err) => {
                        warn!(error = ?err, "oidc discovery failed, retrying");
                        sleep(Duration::from_secs(2)).await;
                    }
                }
            }
        });
    }

    let cors = build_cors_layer(&config.cors_origin)?;
    let app = Router::new()
        .route("/health", get(health))
        .route("/api/oidc/authorize", post(oidc_authorize))
        .route("/oauth/oidc/callback", get(oidc_callback))
        .route("/api/session", get(get_session))
        .route("/api/logout", post(logout))
        .layer(axum::extract::DefaultBodyLimit::max(
            config.oidc_max_request_bytes,
        ))
        .layer(observability::http_trace_layer::<axum::body::Body>())
        .layer(cors)
        .with_state(state);

    let addr = config.web_addr;
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("bind {}", addr))?;
    info!(addr, "auth running");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

async fn oidc_authorize(
    State(state): State<AppState>,
    Json(payload): Json<AuthorizeRequest>,
) -> ApiResult<axum::response::Response> {
    let client = get_client(&state).await?;
    let (auth_url, csrf_token, nonce, pkce_verifier) = client.authorization_url(&state.scopes);
    let state_key = csrf_token.secret().to_string();
    let return_to = crate::browser::return_path(payload.return_to);
    let state_cookie = crate::browser::cookie(&state_key, state.state_ttl, state.cookie_secure)
        .map_err(|_| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "transaction cookie invalid",
            )
        })?;

    let mut pending = state
        .pending
        .lock()
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "state lock failed"))?;
    pending.retain(|_, request| request.created_at.elapsed() < state.state_ttl);
    if pending.len() >= state.max_pending_states {
        return Err(api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "authorization capacity reached",
        ));
    }
    pending.insert(
        state_key,
        PendingState {
            nonce,
            created_at: Instant::now(),
            pkce_verifier,
            return_to,
        },
    );

    let mut response = Json(AuthorizeResponse {
        authorize_url: auth_url,
    })
    .into_response();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, state_cookie);
    Ok(response)
}

async fn oidc_callback(
    State(state): State<AppState>,
    Query(params): Query<CallbackQuery>,
    headers: axum::http::HeaderMap,
) -> ApiResult<impl IntoResponse> {
    if params.error.is_some() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "provider rejected authorization",
        ));
    }

    let code = params
        .code
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "missing oauth code"))?;
    let state_token = params
        .state
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "missing oauth state"))?;
    if !crate::browser::matches(&headers, &state_token, state.cookie_secure) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "authorization browser state mismatch",
        ));
    }
    let pending = {
        let mut guard = state
            .pending
            .lock()
            .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "state lock failed"))?;
        guard.retain(|_, request| request.created_at.elapsed() < state.state_ttl);
        guard
            .remove(&state_token)
            .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "oauth state not found"))?
    };

    let client = get_client(&state).await?;
    let token_response = client
        .oauth
        .exchange_code(AuthorizationCode::new(code.clone()))
        .set_pkce_verifier(pending.pkce_verifier)
        .request_async(&async_http_client)
        .await
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "token exchange failed"))?;
    let id_token = token_response
        .extra_fields()
        .id_token
        .as_deref()
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "missing id_token"))?;
    let claims = client
        .verify(
            id_token,
            pending.nonce.secret(),
            token_response.access_token().secret(),
            &code,
        )
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "id_token invalid"))?;
    let subject = claims.sub;
    let email = claims.email;
    let name = claims.name;

    let session_token = build_session_token(
        &state,
        SessionClaims {
            iss: state.session_issuer.clone(),
            sub: subject.clone(),
            email: email.clone(),
            name: name.clone(),
            exp: 0,
            iat: 0,
        },
    )?;

    let redirect_path = pending
        .return_to
        .clone()
        .unwrap_or_else(|| state.post_login_path.clone());
    let redirect_url = format!("{}{}", state.app_base_url, redirect_path);
    let cookie = build_session_cookie(&session_token, state.session_ttl, state.cookie_secure)?;

    let mut response = Redirect::to(&redirect_url).into_response();
    response.headers_mut().insert(header::SET_COOKIE, cookie);
    let expired =
        crate::browser::cookie("", Duration::ZERO, state.cookie_secure).map_err(|_| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "transaction cookie invalid",
            )
        })?;
    response.headers_mut().append(header::SET_COOKIE, expired);
    Ok(response)
}

async fn get_session(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> ApiResult<Json<SessionResponse>> {
    let token = read_cookie(&headers, session_cookie_name(&state.session_issuer))
        .ok_or_else(|| api_error(StatusCode::UNAUTHORIZED, "missing session"))?;
    let validation = session_validation(&state.session_issuer);
    let data = decode::<SessionClaims>(&token, &state.session_decoding_key, &validation)
        .map_err(|_| api_error(StatusCode::UNAUTHORIZED, "invalid session"))?;
    let expires_at = Utc
        .timestamp_opt(data.claims.exp as i64, 0)
        .single()
        .ok_or_else(|| api_error(StatusCode::INTERNAL_SERVER_ERROR, "invalid session expiry"))?;

    Ok(Json(SessionResponse {
        subject: data.claims.sub,
        email: data.claims.email,
        name: data.claims.name,
        expires_at,
    }))
}

async fn logout(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    let cookie = build_expired_cookie(state.cookie_secure)?;
    let mut response = Json(HealthResponse {
        status: "signed_out".to_string(),
    })
    .into_response();
    response.headers_mut().insert(header::SET_COOKIE, cookie);
    Ok(response)
}

fn build_session_token(state: &AppState, mut claims: SessionClaims) -> ApiResult<String> {
    let now = Utc::now();
    let expires_at = now + chrono::Duration::seconds(state.session_ttl.as_secs() as i64);
    claims.exp = expires_at.timestamp() as usize;
    claims.iat = now.timestamp() as usize;

    encode(&Header::default(), &claims, &state.session_encoding_key)
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "failed to sign session"))
}

fn session_validation(issuer: &str) -> Validation {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[issuer]);
    validation
}

fn build_session_cookie(token: &str, ttl: Duration, secure: bool) -> ApiResult<HeaderValue> {
    let max_age = ttl.as_secs();
    let mut cookie = format!(
        "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        session_cookie_name(if secure { "https://" } else { "http://" }),
        token,
        max_age
    );
    if secure {
        cookie.push_str("; Secure");
    }
    HeaderValue::from_str(&cookie)
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "cookie header invalid"))
}

fn build_expired_cookie(secure: bool) -> ApiResult<HeaderValue> {
    let mut cookie = format!(
        "{}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0",
        session_cookie_name(if secure { "https://" } else { "http://" })
    );
    if secure {
        cookie.push_str("; Secure");
    }
    HeaderValue::from_str(&cookie)
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "cookie header invalid"))
}

fn normalize_path(label: &str, value: &str) -> AnyResult<String> {
    let value = required_trimmed(label, value)?;
    let path = if value.starts_with('/') {
        value
    } else {
        format!("/{}", value)
    };
    ensure!(
        crate::browser::return_path(Some(path.clone())).is_some(),
        "{} must be a safe local path",
        label
    );
    Ok(path)
}

fn parse_scopes(scopes: &str) -> AnyResult<Vec<Scope>> {
    let scopes = scopes
        .split(',')
        .map(|scope| scope.trim())
        .filter(|scope| !scope.is_empty())
        .map(|scope| Scope::new(scope.to_string()))
        .collect::<Vec<_>>();
    ensure!(
        scopes
            .iter()
            .any(|scope: &Scope| scope.as_str() == "openid"),
        "OIDC scopes must include openid"
    );
    Ok(scopes)
}

fn normalize_base_url(label: &str, value: &str) -> AnyResult<String> {
    let value = required_trimmed(label, value)?;
    let value = value.trim_end_matches('/');
    let parsed = oauth2::AuthUrl::new(value.to_owned()).context("invalid application base URL")?;
    let url = parsed.url();
    ensure!(
        url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "application base URL must not contain credentials, query or fragment"
    );
    if url.scheme() == "http" {
        ensure!(
            matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")),
            "HTTP application base URLs are restricted to loopback development"
        );
    }
    ensure!(
        value.starts_with("http://") || value.starts_with("https://"),
        "{} must start with http:// or https://",
        label
    );
    Ok(value.to_string())
}

fn validate_session_ttl(seconds: u64) -> AnyResult<Duration> {
    ensure!(seconds > 0, "session_ttl_secs must be > 0");
    ensure!(seconds <= i64::MAX as u64, "session_ttl_secs is too large");
    Ok(Duration::from_secs(seconds))
}

fn required_trimmed(label: &str, value: &str) -> AnyResult<String> {
    let value = value.trim();
    ensure!(!value.is_empty(), "{} cannot be empty", label);
    Ok(value.to_string())
}

fn build_cors_layer(origins: &[String]) -> AnyResult<CorsLayer> {
    if origins.iter().any(|origin| origin == "*") {
        return Ok(CorsLayer::new()
            .allow_origin(Any)
            .allow_methods([Method::GET, Method::POST])
            .allow_headers(Any));
    }

    let origins = origins
        .iter()
        .map(|origin| HeaderValue::from_str(origin).context("invalid cors origin"))
        .collect::<AnyResult<Vec<_>>>()?;

    Ok(CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
        .allow_credentials(true))
}

async fn build_client(config: &OidcConfig) -> AnyResult<OidcClient> {
    OidcClient::discover(
        required_trimmed("issuer_url", &config.issuer_url)?,
        required_trimmed("client_id", &config.client_id)?,
        config
            .client_secret
            .as_ref()
            .map(|secret| required_trimmed("client_secret", secret))
            .transpose()?,
        config.redirect_url.clone(),
    )
    .await
}

async fn get_client(state: &AppState) -> ApiResult<Arc<OidcClient>> {
    let guard = state.client.read().await;
    guard
        .clone()
        .ok_or_else(|| api_error(StatusCode::SERVICE_UNAVAILABLE, "oidc provider not ready"))
}

fn api_error(status: StatusCode, message: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (
        status,
        Json(ApiError {
            error: message.into(),
        }),
    )
}

#[cfg(test)]
#[path = "browser_handler_tests/mod.rs"]
mod browser_handler_tests;

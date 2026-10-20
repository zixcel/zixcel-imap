//! Web and token refresh service for Checkback IMAP Curator configuration.

mod routes;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use axum::body::Body;
use axum::extract::{Extension, Path as AxumPath, Query, State};
use axum::http::{HeaderValue, Method, Request, StatusCode, header};
use axum::middleware::{Next, from_fn_with_state};
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use checkback_core::RequestContext;
use checkback_core::audit::{AuditRecord, write_audit_log};
use checkback_core::auth::{
    SessionConfig, decode_session, read_session_cookie, session_cookie_name,
};
use checkback_core::tenant::{ensure_subject, resolve_tenant_for_subject};
use chrono::{DateTime, Utc};
use db::{DbConfig, connect_pool, migrate};
use imap_curator_common::openbao::OpenBaoClient;
use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, RedirectUrl, Scope,
    TokenResponse, TokenUrl,
};
use rand::{Rng, distributions::Alphanumeric};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx_core::row::Row;
use tokio::time::sleep;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tracing::{info, warn};
use utoipa::{IntoParams, OpenApi, ToSchema};

use crate::config::{Config, Mode};

#[derive(Debug, Serialize, ToSchema)]
struct AccountsResponse {
    version: u32,
    accounts: Vec<AccountSummary>,
}

#[derive(Debug, Serialize, ToSchema)]
struct AccountSummary {
    account_id: String,
    label: String,
    provider: String,
    username: String,
    enabled: bool,
    auth_type: AuthType,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
enum AuthType {
    Xoauth2,
    Password,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
struct Ms365AuthorizeRequest {
    account_id: String,
    label: String,
    username: String,
    tenant_id: String,
    client_id: String,
    client_secret: String,
    scope: Option<String>,
    enabled: Option<bool>,
    imap_host: Option<String>,
    imap_port: Option<u16>,
}

#[derive(Debug, Serialize, ToSchema)]
struct Ms365AuthorizeResponse {
    authorize_url: String,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
struct IcloudUpsertRequest {
    account_id: String,
    label: String,
    username: String,
    app_password: String,
    enabled: Option<bool>,
    imap_host: Option<String>,
    imap_port: Option<u16>,
}

#[derive(Debug, Serialize, ToSchema)]
struct DomainsResponse {
    version: u32,
    domains: Vec<DomainSummary>,
}

#[derive(Debug, Serialize, ToSchema)]
struct DomainGroupsResponse {
    version: u32,
    total_domains: u64,
    total_entries: u64,
    domains: Vec<DomainGroupNode>,
}

#[derive(Debug, Serialize, ToSchema)]
struct DomainGroupNode {
    domain: String,
    direct_count: u64,
    total_count: u64,
    entries: Vec<DomainSummary>,
    #[schema(no_recursion)]
    children: Vec<DomainGroupNode>,
}

#[derive(Debug, Serialize, ToSchema)]
struct DomainSummary {
    account_id: String,
    account_label: String,
    provider: String,
    mailbox: String,
    domain: String,
    enabled: bool,
    folder: String,
    first_seen: DateTime<Utc>,
    last_seen: DateTime<Utc>,
    count_estimate: u64,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
struct DomainUpdateRequest {
    enabled: bool,
}

#[derive(Debug, Serialize, ToSchema)]
struct ActionResponse {
    status: String,
    message: String,
}

#[derive(Debug, Serialize, ToSchema)]
struct ApiError {
    error: String,
}

#[derive(Debug, Serialize, ToSchema)]
struct StatusResponse {
    version: u32,
    services: Vec<ServiceStatus>,
}

#[derive(Debug, Serialize, ToSchema)]
struct ServiceStatus {
    name: String,
    status: String,
    online: bool,
    last_seen: Option<DateTime<Utc>>,
    interval_secs: Option<u64>,
    detail: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct MailQuery {
    limit: Option<u32>,
    account_id: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
struct MailResponse {
    version: u32,
    mails: Vec<MailSummary>,
}

#[derive(Debug, Serialize, ToSchema)]
struct MailSummary {
    account_id: String,
    account_label: String,
    provider: String,
    mailbox: String,
    uid: u64,
    from_address: Option<String>,
    subject: Option<String>,
    domain: Option<String>,
    collected_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, ToSchema)]
struct MailDomainsResponse {
    version: u32,
    total_domains: u64,
    total_mails: u64,
    domains: Vec<MailDomainNode>,
}

#[derive(Debug, Serialize, ToSchema)]
struct MailDomainNode {
    domain: String,
    direct_count: u64,
    total_count: u64,
    mails: Vec<MailSummary>,
    #[schema(no_recursion)]
    children: Vec<MailDomainNode>,
}

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
#[into_params(parameter_in = Query)]
struct SyncStatusQuery {
    service: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
struct SyncStatusResponse {
    version: u32,
    statuses: Vec<AccountSyncStatus>,
}

#[derive(Debug, Serialize, ToSchema)]
struct AccountSyncStatus {
    account_id: String,
    account_label: String,
    provider: String,
    service: String,
    last_run_at: Option<DateTime<Utc>>,
    last_success_at: Option<DateTime<Utc>>,
    last_error_at: Option<DateTime<Utc>>,
    last_error: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct Ms365CallbackQuery {
    code: String,
    state: String,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_accounts,
        list_domains,
        list_domain_groups,
        list_status,
        list_mail,
        list_mail_domains,
        list_sync_status,
        ms365_authorize,
        ms365_callback,
        icloud_upsert,
        update_domain,
        delete_account,
    ),
    components(schemas(
        AccountsResponse,
        AccountSummary,
        AuthType,
        DomainsResponse,
        DomainSummary,
        DomainGroupsResponse,
        DomainGroupNode,
        DomainUpdateRequest,
        StatusResponse,
        ServiceStatus,
        MailResponse,
        MailSummary,
        MailDomainsResponse,
        MailDomainNode,
        SyncStatusQuery,
        SyncStatusResponse,
        AccountSyncStatus,
        Ms365AuthorizeRequest,
        Ms365AuthorizeResponse,
        IcloudUpsertRequest,
        ActionResponse,
        ApiError,
    )),
    tags(
        (name = "imap-curator-auth-helper", description = "Auth helper configuration API")
    )
)]
struct ApiDoc;

pub async fn run(config: Config) -> Result<()> {
    if matches!(config.mode, Mode::OpenApi) {
        let json = serde_json::to_string_pretty(&ApiDoc::openapi())?;
        println!("{json}");
        return Ok(());
    }
    if config.auth_session_key == "dev-session-key" {
        warn!("AUTH_SESSION_KEY is using the default dev value");
    }
    let session = SessionConfig::new(
        config.auth_session_issuer.clone(),
        config.auth_session_key.clone(),
    );
    let openbao = OpenBaoClient::from_env()?;
    let database_url = config
        .database_url
        .clone()
        .or_else(|| std::env::var("DATABASE_URL").ok())
        .context("DATABASE_URL is not set")?;
    if let Some(openbao) = openbao.as_ref() {
        openbao.wait_ready().await?;
    }
    let pool = connect_pool(&DbConfig::from_url_env(database_url)?).await?;
    migrate(&pool).await?;

    match config.mode {
        Mode::Once => {
            let result = refresh_access_tokens(&pool, openbao.as_ref()).await;
            observability::record_job_result("auth-helper.refresh", result.is_ok());
            result
        }
        Mode::Daemon => loop {
            let result = refresh_access_tokens(&pool, openbao.as_ref()).await;
            observability::record_job_result("auth-helper.refresh", result.is_ok());
            if let Err(err) = result {
                warn!(error = ?err, "imap-curator-auth-helper token refresh failed");
            }
            sleep(Duration::from_secs(config.interval_secs)).await;
        },
        Mode::Web => {
            run_web(
                &config.web_addr,
                &config.cors_origin,
                &config.public_base_url,
                pool,
                openbao,
                session,
            )
            .await
        }
        Mode::OpenApi => Ok(()),
    }
}

async fn run_web(
    addr: &str,
    cors_origins: &[String],
    public_base_url: &str,
    pool: sqlx_postgres::PgPool,
    openbao: Option<OpenBaoClient>,
    session: SessionConfig,
) -> Result<()> {
    let state = AppState {
        pool,
        public_base_url: public_base_url.trim_end_matches('/').to_string(),
        pending: Arc::new(Mutex::new(HashMap::new())),
        openbao,
        session,
    };

    let cors = build_cors_layer(cors_origins)?;
    let api = routes::api().layer(from_fn_with_state(state.clone(), require_session));

    let app = Router::new()
        .merge(api)
        .route("/oauth/ms365/callback", get(ms365_callback))
        .layer(observability::http_trace_layer::<axum::body::Body>())
        .with_state(state)
        .layer(cors);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(addr, "imap-curator-auth-helper web mode running");
    axum::serve(listener, app).await?;
    Ok(())
}

#[tracing::instrument(skip(pool, openbao))]
async fn refresh_access_tokens(
    pool: &sqlx_postgres::PgPool,
    openbao: Option<&OpenBaoClient>,
) -> Result<()> {
    let rows = sqlx_core::query::query(
        r#"SELECT auth.tenant_id, auth.account_id, auth.oauth_tenant_id, auth.client_id,
                  auth.client_secret_ref, auth.refresh_token_ref, auth.access_token_ref, auth.scope
           FROM account_auth auth
           JOIN accounts a ON a.tenant_id = auth.tenant_id AND a.account_id = auth.account_id
           WHERE a.enabled AND auth.auth_type = 'xoauth2' AND auth.refresh_token_ref IS NOT NULL"#,
    )
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok(());
    }
    let client = openbao.context("OpenBao not configured for token refresh")?;
    for row in rows {
        let tenant_id: String = row.get("tenant_id");
        let account_id: String = row.get("account_id");
        let oauth_tenant_id: String = row.try_get("oauth_tenant_id")?;
        let client_id: String = row.try_get("client_id")?;
        let client_secret_ref: String = row.try_get("client_secret_ref")?;
        let refresh_token_ref: String = row.try_get("refresh_token_ref")?;
        let access_token_ref: String = row.try_get("access_token_ref")?;
        let scope: Option<String> = row.try_get("scope")?;
        let client_secret = client.read_value(&client_secret_ref).await?;
        let refresh_token = client.read_value(&refresh_token_ref).await?;
        let access_token = fetch_access_token(
            &oauth_tenant_id,
            &client_id,
            &client_secret,
            &refresh_token,
            scope.as_deref(),
        )
        .await?;
        client.write_value(&access_token_ref, &access_token).await?;
        sqlx_core::query::query(
            "UPDATE account_auth SET updated_at = now() WHERE tenant_id = $1 AND account_id = $2",
        )
        .bind(&tenant_id)
        .bind(&account_id)
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn fetch_access_token(
    oauth_tenant_id: &str,
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
    scope: Option<&str>,
) -> Result<String> {
    let token_url = format!(
        "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
        oauth_tenant_id
    );

    let scope_val = scope.unwrap_or("https://outlook.office365.com/.default");
    let params = [
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("refresh_token", refresh_token),
        ("grant_type", "refresh_token"),
        ("scope", scope_val),
    ];

    let client = reqwest::Client::new();
    let resp = client
        .post(&token_url)
        .form(&params)
        .send()
        .await
        .context("token request failed")?;

    let status = resp.status();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body = resp.text().await.unwrap_or_default();

    if !content_type.starts_with("application/json") {
        anyhow::bail!(
            "token endpoint returned non-json (status={}, content-type={}). body={}",
            status,
            content_type,
            truncate_body(&body)
        );
    }

    if !status.is_success() {
        anyhow::bail!(
            "token endpoint error (status={}). body={}",
            status,
            truncate_body(&body)
        );
    }

    let payload: Value = serde_json::from_str(&body).context("parse token response")?;
    let access_token = payload
        .get("access_token")
        .and_then(|v| v.as_str())
        .context("missing access_token in response")?;
    Ok(access_token.to_string())
}

fn truncate_body(body: &str) -> String {
    let max = 400usize;
    if body.len() <= max {
        body.to_string()
    } else {
        format!("{}...[truncated]", &body[..max])
    }
}

fn openbao_secret_ref(tenant_id: &str, account_id: &str, key: &str) -> Result<String> {
    for identity in [tenant_id, account_id, key] {
        anyhow::ensure!(
            (1..=256).contains(&identity.len())
                && !identity.contains("..")
                && identity.as_bytes()[0].is_ascii_alphanumeric()
                && identity
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')),
            "invalid secret namespace identity"
        );
    }
    Ok(format!(
        "secret/imap-curator/tenants/{}/accounts/{}/{}",
        tenant_id, account_id, key
    ))
}

async fn store_secret(
    openbao: Option<&OpenBaoClient>,
    tenant_id: &str,
    account_id: &str,
    key: &str,
    value: &str,
) -> Result<String> {
    let client = openbao.context("OpenBao not configured for secret storage")?;
    let reference = openbao_secret_ref(tenant_id, account_id, key)?;
    client.write_value(&reference, value).await?;
    Ok(reference)
}

#[derive(Clone)]
struct AppState {
    pool: sqlx_postgres::PgPool,
    public_base_url: String,
    pending: Arc<Mutex<HashMap<String, PendingMs365>>>,
    openbao: Option<OpenBaoClient>,
    session: SessionConfig,
}

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

#[derive(Debug, Clone)]
struct PendingMs365 {
    form: Ms365AuthorizeRequest,
    context: RequestContext,
}

fn api_error(status: StatusCode, message: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (
        status,
        Json(ApiError {
            error: message.into(),
        }),
    )
}

fn build_cors_layer(origins: &[String]) -> Result<CorsLayer> {
    if origins.is_empty() || origins.iter().any(|origin| origin == "*") {
        return Ok(CorsLayer::new()
            .allow_origin(Any)
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PATCH,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers([header::CONTENT_TYPE]));
    }

    let values = origins
        .iter()
        .map(|origin| HeaderValue::from_str(origin).context("invalid cors origin"))
        .collect::<Result<Vec<_>>>()?;
    Ok(CorsLayer::new()
        .allow_origin(AllowOrigin::list(values))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE])
        .allow_credentials(true))
}

async fn require_session(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ApiError>)> {
    let token = read_session_cookie(req.headers(), session_cookie_name(&state.session.issuer))
        .ok_or_else(|| api_error(StatusCode::UNAUTHORIZED, "missing session"))?;
    let claims = decode_session(&token, &state.session)
        .map_err(|_| api_error(StatusCode::UNAUTHORIZED, "invalid session"))?;
    ensure_subject(
        &state.pool,
        &claims.sub,
        claims.email.as_deref(),
        claims.name.as_deref(),
    )
    .await
    .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let display_name = claims.name.as_deref().or(claims.email.as_deref());
    let tenant = resolve_tenant_for_subject(&state.pool, &claims.sub, display_name)
        .await
        .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    req.extensions_mut().insert(RequestContext {
        tenant_id: tenant.tenant_id,
        subject_id: claims.sub,
        subject_email: claims.email,
        subject_name: claims.name,
    });

    Ok(next.run(req).await)
}

#[utoipa::path(
    get,
    path = "/api/accounts",
    responses(
        (status = 200, description = "Account list", body = AccountsResponse),
        (status = 500, description = "Failed to load config", body = ApiError),
    )
)]
async fn list_accounts(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
) -> ApiResult<AccountsResponse> {
    let rows = sqlx_core::query::query(
        r#"
        SELECT
            a.account_id,
            a.label,
            a.provider,
            a.enabled,
            auth.username,
            auth.auth_type
        FROM accounts a
        JOIN account_auth auth ON auth.account_id = a.account_id AND auth.tenant_id = a.tenant_id
        WHERE a.tenant_id = $1
        ORDER BY a.account_id
        "#,
    )
    .bind(&ctx.tenant_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let accounts = rows
        .into_iter()
        .map(|row| {
            let auth_type_str: String = row.get("auth_type");
            let auth_type = match auth_type_str.as_str() {
                "xoauth2" => AuthType::Xoauth2,
                "password" => AuthType::Password,
                other => {
                    tracing::warn!(auth_type = other, "unknown auth_type");
                    AuthType::Password
                }
            };
            AccountSummary {
                account_id: row.get("account_id"),
                label: row.get("label"),
                provider: row.get("provider"),
                username: row.get("username"),
                enabled: row.get("enabled"),
                auth_type,
            }
        })
        .collect();

    Ok(Json(AccountsResponse {
        version: 1,
        accounts,
    }))
}

#[utoipa::path(
    get,
    path = "/api/domains",
    responses(
        (status = 200, description = "Domain list", body = DomainsResponse),
        (status = 500, description = "Failed to load domains", body = ApiError),
    )
)]
async fn list_domains(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
) -> ApiResult<DomainsResponse> {
    let rows = sqlx_core::query::query(
        r#"
        SELECT
            ad.account_id,
            a.label AS account_label,
            a.provider,
            COALESCE(cs.mailbox, m.inbox) AS mailbox,
            ad.domain,
            ad.enabled,
            ad.folder,
            ad.first_seen,
            ad.last_seen,
            ad.count_estimate
        FROM account_domains ad
        JOIN accounts a ON a.account_id = ad.account_id AND a.tenant_id = ad.tenant_id
        LEFT JOIN account_mailboxes m ON m.account_id = ad.account_id AND m.tenant_id = ad.tenant_id
        LEFT JOIN collector_state cs ON cs.account_id = ad.account_id AND cs.tenant_id = ad.tenant_id
        WHERE ad.tenant_id = $1
        ORDER BY ad.account_id, ad.domain
        "#,
    )
    .bind(&ctx.tenant_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let domains = rows
        .into_iter()
        .map(|row| DomainSummary {
            account_id: row.get("account_id"),
            account_label: row.get("account_label"),
            provider: row.get("provider"),
            mailbox: row.get("mailbox"),
            domain: row.get("domain"),
            enabled: row.get("enabled"),
            folder: row.get("folder"),
            first_seen: row.get("first_seen"),
            last_seen: row.get("last_seen"),
            count_estimate: row.get::<i64, _>("count_estimate") as u64,
        })
        .collect();

    Ok(Json(DomainsResponse {
        version: 1,
        domains,
    }))
}

#[utoipa::path(
    get,
    path = "/api/domains/tree",
    responses(
        (status = 200, description = "Domain groups", body = DomainGroupsResponse),
        (status = 500, description = "Failed to load domains", body = ApiError),
    )
)]
async fn list_domain_groups(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
) -> ApiResult<DomainGroupsResponse> {
    let rows = sqlx_core::query::query(
        r#"
        SELECT
            ad.account_id,
            a.label AS account_label,
            a.provider,
            COALESCE(cs.mailbox, m.inbox) AS mailbox,
            ad.domain,
            ad.enabled,
            ad.folder,
            ad.first_seen,
            ad.last_seen,
            ad.count_estimate
        FROM account_domains ad
        JOIN accounts a ON a.account_id = ad.account_id AND a.tenant_id = ad.tenant_id
        LEFT JOIN account_mailboxes m ON m.account_id = ad.account_id AND m.tenant_id = ad.tenant_id
        LEFT JOIN collector_state cs ON cs.account_id = ad.account_id AND cs.tenant_id = ad.tenant_id
        WHERE ad.tenant_id = $1
        ORDER BY ad.account_id, ad.domain
        "#,
    )
    .bind(&ctx.tenant_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let entries = rows
        .into_iter()
        .map(|row| DomainSummary {
            account_id: row.get("account_id"),
            account_label: row.get("account_label"),
            provider: row.get("provider"),
            mailbox: row.get("mailbox"),
            domain: row.get("domain"),
            enabled: row.get("enabled"),
            folder: row.get("folder"),
            first_seen: row.get("first_seen"),
            last_seen: row.get("last_seen"),
            count_estimate: row.get::<i64, _>("count_estimate") as u64,
        })
        .collect::<Vec<_>>();

    let total_entries = entries.len() as u64;
    let (domains, total_domains) = build_domain_group_tree(entries);

    Ok(Json(DomainGroupsResponse {
        version: 1,
        total_domains,
        total_entries,
        domains,
    }))
}

#[utoipa::path(
    get,
    path = "/api/status",
    responses(
        (status = 200, description = "Service status list", body = StatusResponse),
        (status = 500, description = "Failed to load status", body = ApiError),
    )
)]
async fn list_status(
    State(state): State<AppState>,
    Extension(_ctx): Extension<RequestContext>,
) -> ApiResult<StatusResponse> {
    let now = Utc::now();
    let mut services = Vec::new();

    services.push(ServiceStatus {
        name: "imap-curator-auth-helper".to_string(),
        status: "ok".to_string(),
        online: true,
        last_seen: Some(now),
        interval_secs: Some(60),
        detail: Some("api".to_string()),
    });

    let db_status = match sqlx_core::query::query("SELECT 1")
        .fetch_one(&state.pool)
        .await
    {
        Ok(_) => ServiceStatus {
            name: "db".to_string(),
            status: "ok".to_string(),
            online: true,
            last_seen: Some(now),
            interval_secs: None,
            detail: None,
        },
        Err(err) => ServiceStatus {
            name: "db".to_string(),
            status: "error".to_string(),
            online: false,
            last_seen: None,
            interval_secs: None,
            detail: Some(err.to_string()),
        },
    };
    services.push(db_status);

    let openbao_status = if let Some(openbao) = state.openbao.as_ref() {
        match openbao.health_status().await {
            Ok(status) => ServiceStatus {
                name: "openbao".to_string(),
                status: if status.is_success() { "ok" } else { "error" }.to_string(),
                online: status.is_success(),
                last_seen: Some(now),
                interval_secs: None,
                detail: Some(format!("status {}", status)),
            },
            Err(err) => ServiceStatus {
                name: "openbao".to_string(),
                status: "error".to_string(),
                online: false,
                last_seen: None,
                interval_secs: None,
                detail: Some(err.to_string()),
            },
        }
    } else {
        ServiceStatus {
            name: "openbao".to_string(),
            status: "error".to_string(),
            online: false,
            last_seen: None,
            interval_secs: None,
            detail: Some("OpenBao not configured".to_string()),
        }
    };
    services.push(openbao_status);

    let status_rows = sqlx_core::query::query(
        "SELECT service, last_seen, interval_secs, detail FROM service_status WHERE service IN ('collector','operator')",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let mut status_map = HashMap::new();
    for row in status_rows {
        let service: String = row.get("service");
        let last_seen: DateTime<Utc> = row.get("last_seen");
        let interval_secs: i32 = row.get("interval_secs");
        let detail: Option<String> = row.try_get("detail").ok();
        status_map.insert(service, (last_seen, interval_secs as i64, detail));
    }

    for name in ["collector", "operator"] {
        let entry = status_map.get(name);
        if let Some((last_seen, interval_secs, detail)) = entry {
            let elapsed = now.signed_duration_since(*last_seen).num_seconds();
            let threshold = interval_secs.saturating_mul(2).max(30);
            let online = elapsed <= threshold;
            services.push(ServiceStatus {
                name: name.to_string(),
                status: if online { "ok" } else { "stale" }.to_string(),
                online,
                last_seen: Some(*last_seen),
                interval_secs: Some(*interval_secs as u64),
                detail: detail.clone(),
            });
        } else {
            services.push(ServiceStatus {
                name: name.to_string(),
                status: "unknown".to_string(),
                online: false,
                last_seen: None,
                interval_secs: None,
                detail: Some("no heartbeat yet".to_string()),
            });
        }
    }

    Ok(Json(StatusResponse {
        version: 1,
        services,
    }))
}

#[utoipa::path(
    get,
    path = "/api/mail",
    params(MailQuery),
    responses(
        (status = 200, description = "Mail list", body = MailResponse),
        (status = 500, description = "Failed to load mail", body = ApiError),
    )
)]
async fn list_mail(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Query(query): Query<MailQuery>,
) -> ApiResult<MailResponse> {
    let limit = query.limit.unwrap_or(200).min(500) as i64;
    let account_id = query.account_id.as_deref();
    let rows = sqlx_core::query::query(
        r#"
        SELECT
            m.account_id,
            a.label AS account_label,
            a.provider,
            m.mailbox,
            m.uid,
            m.from_address,
            m.subject,
            m.domain,
            m.collected_at
        FROM mail_messages m
        JOIN accounts a ON a.account_id = m.account_id AND a.tenant_id = m.tenant_id
        WHERE m.tenant_id = $1 AND ($2::text IS NULL OR m.account_id = $2)
        ORDER BY m.collected_at DESC
        LIMIT $3
        "#,
    )
    .bind(&ctx.tenant_id)
    .bind(account_id)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let mails = rows
        .into_iter()
        .map(|row| MailSummary {
            account_id: row.get("account_id"),
            account_label: row.get("account_label"),
            provider: row.get("provider"),
            mailbox: row.get("mailbox"),
            uid: row.get::<i64, _>("uid") as u64,
            from_address: row.try_get("from_address").ok(),
            subject: row.try_get("subject").ok(),
            domain: row.try_get("domain").ok(),
            collected_at: row.get("collected_at"),
        })
        .collect();

    Ok(Json(MailResponse { version: 1, mails }))
}

#[utoipa::path(
    get,
    path = "/api/mail/domains",
    params(MailQuery),
    responses(
        (status = 200, description = "Mail domains tree", body = MailDomainsResponse),
        (status = 500, description = "Failed to load mail", body = ApiError),
    )
)]
async fn list_mail_domains(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Query(query): Query<MailQuery>,
) -> ApiResult<MailDomainsResponse> {
    let limit = query.limit.unwrap_or(2000).min(10000) as i64;
    let account_id = query.account_id.as_deref();
    let rows = sqlx_core::query::query(
        r#"
        SELECT
            m.account_id,
            a.label AS account_label,
            a.provider,
            m.mailbox,
            m.uid,
            m.from_address,
            m.subject,
            m.domain,
            m.collected_at
        FROM mail_messages m
        JOIN accounts a ON a.account_id = m.account_id AND a.tenant_id = m.tenant_id
        WHERE m.tenant_id = $1 AND ($2::text IS NULL OR m.account_id = $2)
        ORDER BY m.collected_at DESC
        LIMIT $3
        "#,
    )
    .bind(&ctx.tenant_id)
    .bind(account_id)
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let mails = rows
        .into_iter()
        .map(|row| MailSummary {
            account_id: row.get("account_id"),
            account_label: row.get("account_label"),
            provider: row.get("provider"),
            mailbox: row.get("mailbox"),
            uid: row.get::<i64, _>("uid") as u64,
            from_address: row.try_get("from_address").ok(),
            subject: row.try_get("subject").ok(),
            domain: row.try_get("domain").ok(),
            collected_at: row.get("collected_at"),
        })
        .collect::<Vec<_>>();

    let total_mails = mails.len() as u64;
    let (domains, total_domains) = build_mail_domain_tree(mails);

    Ok(Json(MailDomainsResponse {
        version: 1,
        total_domains,
        total_mails,
        domains,
    }))
}

#[utoipa::path(
    get,
    path = "/api/sync-status",
    params(SyncStatusQuery),
    responses(
        (status = 200, description = "Account sync status list", body = SyncStatusResponse),
        (status = 500, description = "Failed to load sync status", body = ApiError),
    )
)]
async fn list_sync_status(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Query(query): Query<SyncStatusQuery>,
) -> ApiResult<SyncStatusResponse> {
    let service = query
        .service
        .clone()
        .unwrap_or_else(|| "collector".to_string());
    let rows = sqlx_core::query::query(
        r#"
        SELECT
            a.account_id,
            a.label AS account_label,
            a.provider,
            s.service,
            s.last_run_at,
            s.last_success_at,
            s.last_error_at,
            s.last_error
        FROM accounts a
        LEFT JOIN account_sync_status s
            ON s.account_id = a.account_id AND s.tenant_id = a.tenant_id AND s.service = $1
        WHERE a.tenant_id = $2
        ORDER BY a.account_id
        "#,
    )
    .bind(&service)
    .bind(&ctx.tenant_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let statuses = rows
        .into_iter()
        .map(|row| AccountSyncStatus {
            account_id: row.get("account_id"),
            account_label: row.get("account_label"),
            provider: row.get("provider"),
            service: row
                .try_get::<String, _>("service")
                .unwrap_or_else(|_| service.clone()),
            last_run_at: row.try_get("last_run_at").ok(),
            last_success_at: row.try_get("last_success_at").ok(),
            last_error_at: row.try_get("last_error_at").ok(),
            last_error: row.try_get("last_error").ok(),
        })
        .collect();

    Ok(Json(SyncStatusResponse {
        version: 1,
        statuses,
    }))
}

#[derive(Debug)]
struct DomainGroupNodeBuilder {
    domain: String,
    direct_count: u64,
    total_count: u64,
    entries: Vec<DomainSummary>,
    children: HashMap<String, DomainGroupNodeBuilder>,
}

impl DomainGroupNodeBuilder {
    fn new(domain: String) -> Self {
        Self {
            domain,
            direct_count: 0,
            total_count: 0,
            entries: Vec::new(),
            children: HashMap::new(),
        }
    }
}

fn build_domain_group_tree(entries: Vec<DomainSummary>) -> (Vec<DomainGroupNode>, u64) {
    let mut roots: HashMap<String, DomainGroupNodeBuilder> = HashMap::new();
    let mut domain_count = 0u64;

    for mut entry in entries {
        let normalized = normalize_domain(Some(&entry.domain));
        entry.domain = normalized.clone();
        let chain = domain_chain(&normalized);
        insert_domain_entry(&mut roots, &chain, entry, &mut domain_count);
    }

    let mut nodes = finalize_domain_nodes(roots);
    nodes.sort_by(|a, b| a.domain.cmp(&b.domain));
    (nodes, domain_count)
}

fn insert_domain_entry(
    roots: &mut HashMap<String, DomainGroupNodeBuilder>,
    chain: &[String],
    entry: DomainSummary,
    domain_count: &mut u64,
) {
    let mut current = roots;
    for (index, domain) in chain.iter().enumerate() {
        let is_leaf = index + 1 == chain.len();
        let node = current.entry(domain.clone()).or_insert_with(|| {
            *domain_count = domain_count.saturating_add(1);
            DomainGroupNodeBuilder::new(domain.clone())
        });
        node.total_count = node.total_count.saturating_add(1);
        if is_leaf {
            node.direct_count = node.direct_count.saturating_add(1);
            node.entries.push(entry);
            break;
        }
        current = &mut node.children;
    }
}

fn finalize_domain_nodes(nodes: HashMap<String, DomainGroupNodeBuilder>) -> Vec<DomainGroupNode> {
    let mut list = nodes
        .into_values()
        .map(finalize_domain_node)
        .collect::<Vec<_>>();
    list.sort_by(|a, b| a.domain.cmp(&b.domain));
    list
}

fn finalize_domain_node(node: DomainGroupNodeBuilder) -> DomainGroupNode {
    let children = finalize_domain_nodes(node.children);
    DomainGroupNode {
        domain: node.domain,
        direct_count: node.direct_count,
        total_count: node.total_count,
        entries: node.entries,
        children,
    }
}

#[derive(Debug)]
struct MailDomainNodeBuilder {
    domain: String,
    direct_count: u64,
    total_count: u64,
    mails: Vec<MailSummary>,
    children: HashMap<String, MailDomainNodeBuilder>,
}

impl MailDomainNodeBuilder {
    fn new(domain: String) -> Self {
        Self {
            domain,
            direct_count: 0,
            total_count: 0,
            mails: Vec::new(),
            children: HashMap::new(),
        }
    }
}

fn build_mail_domain_tree(mails: Vec<MailSummary>) -> (Vec<MailDomainNode>, u64) {
    let mut roots: HashMap<String, MailDomainNodeBuilder> = HashMap::new();
    let mut domain_count = 0u64;

    for mail in mails {
        let domain = normalize_domain(mail.domain.as_deref());
        let chain = domain_chain(&domain);
        let mut mail = mail;
        mail.domain = Some(domain);
        insert_mail(&mut roots, &chain, mail, &mut domain_count);
    }

    let mut nodes = finalize_nodes(roots);
    nodes.sort_by(|a, b| a.domain.cmp(&b.domain));
    (nodes, domain_count)
}

fn insert_mail(
    roots: &mut HashMap<String, MailDomainNodeBuilder>,
    chain: &[String],
    mail: MailSummary,
    domain_count: &mut u64,
) {
    let mut current = roots;
    for (index, domain) in chain.iter().enumerate() {
        let is_leaf = index + 1 == chain.len();
        let node = current.entry(domain.clone()).or_insert_with(|| {
            *domain_count = domain_count.saturating_add(1);
            MailDomainNodeBuilder::new(domain.clone())
        });
        node.total_count = node.total_count.saturating_add(1);
        if is_leaf {
            node.direct_count = node.direct_count.saturating_add(1);
            node.mails.push(mail);
            break;
        }
        current = &mut node.children;
    }
}

fn finalize_nodes(nodes: HashMap<String, MailDomainNodeBuilder>) -> Vec<MailDomainNode> {
    let mut list = nodes.into_values().map(finalize_node).collect::<Vec<_>>();
    list.sort_by(|a, b| a.domain.cmp(&b.domain));
    list
}

fn finalize_node(node: MailDomainNodeBuilder) -> MailDomainNode {
    let children = finalize_nodes(node.children);
    MailDomainNode {
        domain: node.domain,
        direct_count: node.direct_count,
        total_count: node.total_count,
        mails: node.mails,
        children,
    }
}

fn normalize_domain(value: Option<&str>) -> String {
    let domain = value.unwrap_or("unknown").trim().to_lowercase();
    if domain.is_empty() {
        "unknown".to_string()
    } else {
        domain
    }
}

fn domain_chain(domain: &str) -> Vec<String> {
    let parts: Vec<&str> = domain.split('.').filter(|part| !part.is_empty()).collect();
    if parts.len() <= 1 {
        return vec![domain.to_string()];
    }

    let mut chain = Vec::new();
    let mut current = format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1]);
    chain.push(current.clone());

    if parts.len() > 2 {
        for index in (0..parts.len() - 2).rev() {
            current = format!("{}.{}", parts[index], current);
            chain.push(current.clone());
        }
    }

    chain
}

#[utoipa::path(
    post,
    path = "/api/accounts/ms365/authorize",
    request_body = Ms365AuthorizeRequest,
    responses(
        (status = 200, description = "Authorize URL created", body = Ms365AuthorizeResponse),
        (status = 400, description = "Invalid request", body = ApiError),
        (status = 500, description = "Failed to prepare OAuth request", body = ApiError),
    )
)]
async fn ms365_authorize(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Json(form): Json<Ms365AuthorizeRequest>,
) -> ApiResult<Ms365AuthorizeResponse> {
    let state_token: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();

    {
        let mut pending = state
            .pending
            .lock()
            .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "state lock failed"))?;
        pending.insert(
            state_token.clone(),
            PendingMs365 {
                form: form.clone(),
                context: ctx.clone(),
            },
        );
    }

    let redirect_url = oauth_redirect_url(&state.public_base_url)
        .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let client = build_oauth_client(
        &form.tenant_id,
        &form.client_id,
        &form.client_secret,
        redirect_url,
    )
    .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let mut auth_req = client
        .authorize_url(|| CsrfToken::new(state_token))
        .add_scope(Scope::new("offline_access".to_string()));
    let scope_val = form
        .scope
        .clone()
        .unwrap_or_else(|| "https://outlook.office365.com/.default".to_string());
    auth_req = auth_req.add_scope(Scope::new(scope_val));
    let (auth_url, _csrf_token) = auth_req.url();

    Ok(Json(Ms365AuthorizeResponse {
        authorize_url: auth_url.as_str().to_string(),
    }))
}

#[utoipa::path(
    get,
    path = "/oauth/ms365/callback",
    params(Ms365CallbackQuery),
    responses(
        (status = 200, description = "OAuth callback processed", body = ActionResponse),
        (status = 400, description = "OAuth callback error", body = ApiError),
        (status = 500, description = "Failed to save config", body = ApiError),
    )
)]
async fn ms365_callback(
    State(state): State<AppState>,
    Query(query): Query<Ms365CallbackQuery>,
) -> ApiResult<ActionResponse> {
    if let Some(err) = query.error {
        let desc = query.error_description.unwrap_or_default();
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            format!("oauth error: {} {}", err, desc),
        ));
    }

    let pending = {
        let mut pending = state
            .pending
            .lock()
            .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "state lock failed"))?;
        pending.remove(&query.state)
    };

    let Some(pending) = pending else {
        return Err(api_error(StatusCode::BAD_REQUEST, "missing oauth state"));
    };

    let redirect_url = oauth_redirect_url(&state.public_base_url)
        .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let client = build_oauth_client(
        &pending.form.tenant_id,
        &pending.form.client_id,
        &pending.form.client_secret,
        redirect_url,
    )
    .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let token = client
        .exchange_code(AuthorizationCode::new(query.code))
        .request_async(oauth_transport::async_http_client)
        .await
        .map_err(|err| {
            api_error(
                StatusCode::BAD_REQUEST,
                format!("token exchange failed: {}", err),
            )
        })?;

    let refresh = token.refresh_token().ok_or_else(|| {
        api_error(
            StatusCode::BAD_REQUEST,
            "refresh_token missing; ensure offline_access scope",
        )
    })?;

    if let Err(err) = upsert_ms365_account(
        &state.pool,
        state.openbao.as_ref(),
        &pending.context,
        &pending.form,
        refresh.secret(),
    )
    .await
    {
        let record = AuditRecord {
            tenant_id: Some(pending.context.tenant_id.clone()),
            subject_id: Some(pending.context.subject_id.clone()),
            subject_email: pending.context.subject_email.clone(),
            action: "account.ms365.upsert".to_string(),
            resource_type: Some("account".to_string()),
            resource_id: Some(pending.form.account_id.clone()),
            success: false,
            error_code: Some("upsert_failed".to_string()),
            detail: Some(json!({ "error": err.to_string() })),
        };
        let _ = write_audit_log(&state.pool, &record).await;
        return Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            err.to_string(),
        ));
    }

    Ok(Json(ActionResponse {
        status: "ok".to_string(),
        message: "MS365 config saved. You can close this tab.".to_string(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/accounts/icloud",
    request_body = IcloudUpsertRequest,
    responses(
        (status = 200, description = "iCloud config saved", body = ActionResponse),
        (status = 500, description = "Failed to save config", body = ApiError),
    )
)]
async fn icloud_upsert(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    Json(form): Json<IcloudUpsertRequest>,
) -> ApiResult<ActionResponse> {
    if let Err(err) = upsert_icloud_account(&state.pool, state.openbao.as_ref(), &ctx, &form).await
    {
        let record = AuditRecord {
            tenant_id: Some(ctx.tenant_id.clone()),
            subject_id: Some(ctx.subject_id.clone()),
            subject_email: ctx.subject_email.clone(),
            action: "account.icloud.upsert".to_string(),
            resource_type: Some("account".to_string()),
            resource_id: Some(form.account_id.clone()),
            success: false,
            error_code: Some("upsert_failed".to_string()),
            detail: Some(json!({ "error": err.to_string() })),
        };
        let _ = write_audit_log(&state.pool, &record).await;
        return Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            err.to_string(),
        ));
    }

    Ok(Json(ActionResponse {
        status: "ok".to_string(),
        message: "iCloud config saved. You can close this tab.".to_string(),
    }))
}

#[utoipa::path(
    patch,
    path = "/api/domains/{account_id}/{domain}",
    request_body = DomainUpdateRequest,
    params(
        ("account_id" = String, Path, description = "Account id"),
        ("domain" = String, Path, description = "Domain name")
    ),
    responses(
        (status = 200, description = "Domain updated", body = ActionResponse),
        (status = 404, description = "Domain not found", body = ApiError),
        (status = 500, description = "Failed to update domain", body = ApiError),
    )
)]
async fn update_domain(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    AxumPath((account_id, domain)): AxumPath<(String, String)>,
    Json(payload): Json<DomainUpdateRequest>,
) -> ApiResult<ActionResponse> {
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let result = sqlx_core::query::query(
        "UPDATE account_domains SET enabled = $1 WHERE tenant_id = $2 AND account_id = $3 AND domain = $4",
    )
    .bind(payload.enabled)
    .bind(&ctx.tenant_id)
    .bind(&account_id)
    .bind(&domain)
    .execute(&mut *tx)
    .await
    .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    if result.rows_affected() == 0 {
        let record = AuditRecord {
            tenant_id: Some(ctx.tenant_id.clone()),
            subject_id: Some(ctx.subject_id.clone()),
            subject_email: ctx.subject_email.clone(),
            action: "domain.update".to_string(),
            resource_type: Some("domain".to_string()),
            resource_id: Some(format!("{}:{}", account_id, domain)),
            success: false,
            error_code: Some("not_found".to_string()),
            detail: None,
        };
        write_audit_log(&mut *tx, &record)
            .await
            .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
        tx.commit()
            .await
            .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
        return Err(api_error(StatusCode::NOT_FOUND, "domain not found"));
    }

    let record = AuditRecord {
        tenant_id: Some(ctx.tenant_id.clone()),
        subject_id: Some(ctx.subject_id.clone()),
        subject_email: ctx.subject_email.clone(),
        action: "domain.update".to_string(),
        resource_type: Some("domain".to_string()),
        resource_id: Some(format!("{}:{}", account_id, domain)),
        success: true,
        error_code: None,
        detail: Some(json!({ "enabled": payload.enabled })),
    };
    write_audit_log(&mut *tx, &record)
        .await
        .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    tx.commit()
        .await
        .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    Ok(Json(ActionResponse {
        status: "ok".to_string(),
        message: "Domain updated.".to_string(),
    }))
}

#[utoipa::path(
    delete,
    path = "/api/accounts/{account_id}",
    params(
        ("account_id" = String, Path, description = "Account id")
    ),
    responses(
        (status = 200, description = "Account deleted", body = ActionResponse),
        (status = 404, description = "Account not found", body = ApiError),
        (status = 500, description = "Failed to delete account", body = ApiError),
    )
)]
async fn delete_account(
    State(state): State<AppState>,
    Extension(ctx): Extension<RequestContext>,
    AxumPath(account_id): AxumPath<String>,
) -> ApiResult<ActionResponse> {
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    let result =
        sqlx_core::query::query("DELETE FROM accounts WHERE tenant_id = $1 AND account_id = $2")
            .bind(&ctx.tenant_id)
            .bind(&account_id)
            .execute(&mut *tx)
            .await
            .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    if result.rows_affected() == 0 {
        let record = AuditRecord {
            tenant_id: Some(ctx.tenant_id.clone()),
            subject_id: Some(ctx.subject_id.clone()),
            subject_email: ctx.subject_email.clone(),
            action: "account.delete".to_string(),
            resource_type: Some("account".to_string()),
            resource_id: Some(account_id.clone()),
            success: false,
            error_code: Some("not_found".to_string()),
            detail: None,
        };
        write_audit_log(&mut *tx, &record)
            .await
            .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
        tx.commit()
            .await
            .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
        return Err(api_error(StatusCode::NOT_FOUND, "account_id not found"));
    }

    let record = AuditRecord {
        tenant_id: Some(ctx.tenant_id.clone()),
        subject_id: Some(ctx.subject_id.clone()),
        subject_email: ctx.subject_email.clone(),
        action: "account.delete".to_string(),
        resource_type: Some("account".to_string()),
        resource_id: Some(account_id.clone()),
        success: true,
        error_code: None,
        detail: None,
    };
    write_audit_log(&mut *tx, &record)
        .await
        .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    tx.commit()
        .await
        .map_err(|err| api_error(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    Ok(Json(ActionResponse {
        status: "ok".to_string(),
        message: "Account deleted. Reload the page.".to_string(),
    }))
}

#[tracing::instrument(skip(pool, openbao, form, refresh_token))]
async fn upsert_ms365_account(
    pool: &sqlx_postgres::PgPool,
    openbao: Option<&OpenBaoClient>,
    context: &RequestContext,
    form: &Ms365AuthorizeRequest,
    refresh_token: &str,
) -> Result<()> {
    let openbao = Some(openbao.context("OpenBao not configured for account secret storage")?);
    let enabled = form.enabled.unwrap_or(false);
    let imap_host = form
        .imap_host
        .clone()
        .unwrap_or_else(|| "outlook.office365.com".to_string());
    let imap_port = form.imap_port.unwrap_or(993);
    let access_token = fetch_access_token(
        &form.tenant_id,
        &form.client_id,
        &form.client_secret,
        refresh_token,
        form.scope.as_deref(),
    )
    .await?;

    let client_secret_ref = store_secret(
        openbao,
        &context.tenant_id,
        &form.account_id,
        "client_secret",
        &form.client_secret,
    )
    .await?;
    let refresh_token_ref = store_secret(
        openbao,
        &context.tenant_id,
        &form.account_id,
        "refresh_token",
        refresh_token,
    )
    .await?;
    let access_token_ref = store_secret(
        openbao,
        &context.tenant_id,
        &form.account_id,
        "access_token",
        &access_token,
    )
    .await?;

    let mut tx = pool.begin().await?;

    sqlx_core::query::query(
        r#"
        INSERT INTO accounts (tenant_id, account_id, label, provider, enabled)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (tenant_id, account_id) DO UPDATE SET
            label = EXCLUDED.label,
            provider = EXCLUDED.provider,
            enabled = EXCLUDED.enabled,
            updated_at = now()
        "#,
    )
    .bind(&context.tenant_id)
    .bind(&form.account_id)
    .bind(&form.label)
    .bind("microsoft")
    .bind(enabled)
    .execute(&mut *tx)
    .await?;

    sqlx_core::query::query(
        r#"
        INSERT INTO account_imap (tenant_id, account_id, host, port, tls_mode, tls_verify)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (tenant_id, account_id) DO UPDATE SET
            host = EXCLUDED.host,
            port = EXCLUDED.port,
            tls_mode = EXCLUDED.tls_mode,
            tls_verify = EXCLUDED.tls_verify
        "#,
    )
    .bind(&context.tenant_id)
    .bind(&form.account_id)
    .bind(imap_host)
    .bind(imap_port as i32)
    .bind("implicit")
    .bind(true)
    .execute(&mut *tx)
    .await?;

    sqlx_core::query::query(
        r#"
        INSERT INTO account_mailboxes (tenant_id, account_id, inbox)
        VALUES ($1, $2, $3)
        ON CONFLICT (tenant_id, account_id) DO UPDATE SET
            inbox = EXCLUDED.inbox
        "#,
    )
    .bind(&context.tenant_id)
    .bind(&form.account_id)
    .bind("INBOX")
    .execute(&mut *tx)
    .await?;

    sqlx_core::query::query(
        r#"
        INSERT INTO account_limits (tenant_id, account_id, search_page_size, move_batch_size)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (tenant_id, account_id) DO UPDATE SET
            search_page_size = EXCLUDED.search_page_size,
            move_batch_size = EXCLUDED.move_batch_size
        "#,
    )
    .bind(&context.tenant_id)
    .bind(&form.account_id)
    .bind(5000)
    .bind(200)
    .execute(&mut *tx)
    .await?;

    sqlx_core::query::query(
        r#"
        INSERT INTO account_auth
            (
                tenant_id,
                account_id,
                auth_type,
                username,
                oauth_tenant_id,
                client_id,
                client_secret_ref,
                refresh_token_ref,
                scope,
                access_token_ref
            )
        VALUES ($1, $2, 'xoauth2', $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (tenant_id, account_id) DO UPDATE SET
            auth_type = EXCLUDED.auth_type,
            username = EXCLUDED.username,
            oauth_tenant_id = EXCLUDED.oauth_tenant_id,
            client_id = EXCLUDED.client_id,
            client_secret_ref = EXCLUDED.client_secret_ref,
            refresh_token_ref = EXCLUDED.refresh_token_ref,
            scope = EXCLUDED.scope,
            access_token_ref = EXCLUDED.access_token_ref,
            updated_at = now()
        "#,
    )
    .bind(&context.tenant_id)
    .bind(&form.account_id)
    .bind(&form.username)
    .bind(&form.tenant_id)
    .bind(&form.client_id)
    .bind(client_secret_ref)
    .bind(refresh_token_ref)
    .bind(&form.scope)
    .bind(access_token_ref)
    .execute(&mut *tx)
    .await?;

    let record = AuditRecord {
        tenant_id: Some(context.tenant_id.clone()),
        subject_id: Some(context.subject_id.clone()),
        subject_email: context.subject_email.clone(),
        action: "account.ms365.upsert".to_string(),
        resource_type: Some("account".to_string()),
        resource_id: Some(form.account_id.clone()),
        success: true,
        error_code: None,
        detail: Some(json!({
            "provider": "microsoft",
            "username": form.username.clone(),
            "enabled": enabled
        })),
    };
    write_audit_log(&mut *tx, &record).await?;

    tx.commit().await?;
    Ok(())
}

#[tracing::instrument(skip(pool, openbao, form))]
async fn upsert_icloud_account(
    pool: &sqlx_postgres::PgPool,
    openbao: Option<&OpenBaoClient>,
    context: &RequestContext,
    form: &IcloudUpsertRequest,
) -> Result<()> {
    let openbao = Some(openbao.context("OpenBao not configured for account secret storage")?);
    let enabled = form.enabled.unwrap_or(false);
    let imap_host = form
        .imap_host
        .clone()
        .unwrap_or_else(|| "imap.mail.me.com".to_string());
    let imap_port = form.imap_port.unwrap_or(993);
    let username = form.username.trim().to_string();
    let app_password = form.app_password.replace(char::is_whitespace, "");
    let app_password_ref = store_secret(
        openbao,
        &context.tenant_id,
        &form.account_id,
        "app_password",
        &app_password,
    )
    .await?;

    let mut tx = pool.begin().await?;

    sqlx_core::query::query(
        r#"
        INSERT INTO accounts (tenant_id, account_id, label, provider, enabled)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (tenant_id, account_id) DO UPDATE SET
            label = EXCLUDED.label,
            provider = EXCLUDED.provider,
            enabled = EXCLUDED.enabled,
            updated_at = now()
        "#,
    )
    .bind(&context.tenant_id)
    .bind(&form.account_id)
    .bind(&form.label)
    .bind("apple")
    .bind(enabled)
    .execute(&mut *tx)
    .await?;

    sqlx_core::query::query(
        r#"
        INSERT INTO account_imap (tenant_id, account_id, host, port, tls_mode, tls_verify)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (tenant_id, account_id) DO UPDATE SET
            host = EXCLUDED.host,
            port = EXCLUDED.port,
            tls_mode = EXCLUDED.tls_mode,
            tls_verify = EXCLUDED.tls_verify
        "#,
    )
    .bind(&context.tenant_id)
    .bind(&form.account_id)
    .bind(imap_host)
    .bind(imap_port as i32)
    .bind("implicit")
    .bind(true)
    .execute(&mut *tx)
    .await?;

    sqlx_core::query::query(
        r#"
        INSERT INTO account_mailboxes (tenant_id, account_id, inbox)
        VALUES ($1, $2, $3)
        ON CONFLICT (tenant_id, account_id) DO UPDATE SET
            inbox = EXCLUDED.inbox
        "#,
    )
    .bind(&context.tenant_id)
    .bind(&form.account_id)
    .bind("INBOX")
    .execute(&mut *tx)
    .await?;

    sqlx_core::query::query(
        r#"
        INSERT INTO account_limits (tenant_id, account_id, search_page_size, move_batch_size)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (tenant_id, account_id) DO UPDATE SET
            search_page_size = EXCLUDED.search_page_size,
            move_batch_size = EXCLUDED.move_batch_size
        "#,
    )
    .bind(&context.tenant_id)
    .bind(&form.account_id)
    .bind(5000)
    .bind(200)
    .execute(&mut *tx)
    .await?;

    sqlx_core::query::query(
        r#"
        INSERT INTO account_auth (tenant_id, account_id, auth_type, username, app_password_ref)
        VALUES ($1, $2, 'password', $3, $4)
        ON CONFLICT (tenant_id, account_id) DO UPDATE SET
            auth_type = EXCLUDED.auth_type,
            username = EXCLUDED.username,
            app_password_ref = EXCLUDED.app_password_ref,
            updated_at = now()
        "#,
    )
    .bind(&context.tenant_id)
    .bind(&form.account_id)
    .bind(&username)
    .bind(app_password_ref)
    .execute(&mut *tx)
    .await?;

    let record = AuditRecord {
        tenant_id: Some(context.tenant_id.clone()),
        subject_id: Some(context.subject_id.clone()),
        subject_email: context.subject_email.clone(),
        action: "account.icloud.upsert".to_string(),
        resource_type: Some("account".to_string()),
        resource_id: Some(form.account_id.clone()),
        success: true,
        error_code: None,
        detail: Some(json!({
            "provider": "apple",
            "username": username.clone(),
            "enabled": enabled
        })),
    };
    write_audit_log(&mut *tx, &record).await?;

    tx.commit().await?;
    Ok(())
}

fn build_oauth_client(
    tenant_id: &str,
    client_id: &str,
    client_secret: &str,
    redirect_url: RedirectUrl,
) -> Result<BasicClient> {
    let auth_url = AuthUrl::new(format!(
        "https://login.microsoftonline.com/{}/oauth2/v2.0/authorize",
        tenant_id
    ))?;
    let token_url = TokenUrl::new(format!(
        "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
        tenant_id
    ))?;

    let client = BasicClient::new(
        ClientId::new(client_id.to_string()),
        Some(ClientSecret::new(client_secret.to_string())),
        auth_url,
        Some(token_url),
    )
    .set_redirect_uri(redirect_url);

    Ok(client)
}

fn oauth_redirect_url(public_base_url: &str) -> Result<RedirectUrl> {
    let base = public_base_url.trim_end_matches('/');
    RedirectUrl::new(format!("{}/oauth/ms365/callback", base)).context("invalid public base url")
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};

    #[test]
    fn secret_namespace_identity_cannot_escape_its_tenant() {
        for identity in [
            "",
            "../other",
            "other/account",
            "..",
            "fixture\nother",
            "%2fother",
            " fixture",
        ] {
            assert!(openbao_secret_ref(identity, "fixture-account", "access_token").is_err());
            assert!(openbao_secret_ref("fixture-a", identity, "access_token").is_err());
        }
    }

    #[tokio::test]
    async fn secret_storage_requires_the_provider() {
        assert!(
            store_secret(
                None,
                "fixture-a",
                "fixture-account",
                "access_token",
                "synthetic"
            )
            .await
            .is_err()
        );
    }

    #[test]
    fn cli_requires_explicit_session_key_and_defaults_to_loopback() {
        let missing = Config::command()
            .mut_arg("auth_session_key", |arg| arg.env(None::<&str>))
            .try_get_matches_from(["imap-curator-auth-helper"])
            .expect_err("session key is required");
        assert_eq!(
            missing.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
        let config = Config::parse_from([
            "imap-curator-auth-helper",
            "--auth-session-key",
            "fixture-only-invalid-session-key",
        ]);
        assert!(matches!(config.mode, Mode::Web));
        assert_eq!(config.interval_secs, 3600);
        assert_eq!(config.web_addr, "127.0.0.1:8080");
        assert_eq!(
            config.cors_origin,
            vec!["http://localhost:3000".to_string()]
        );
        assert_eq!(config.public_base_url, "http://localhost:3000");
        assert_eq!(config.auth_session_key, "fixture-only-invalid-session-key");
        assert_eq!(config.auth_session_issuer, "http://localhost:8090");
        assert!(config.database_url.is_none());
    }
}

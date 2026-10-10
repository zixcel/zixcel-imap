use clap::Parser;

#[derive(Parser)]
#[command(
    name = "auth",
    version,
    about = "OIDC authentication service for Checkback"
)]
pub struct Config {
    #[arg(long, env = "AUTH_WEB_ADDR", default_value = "127.0.0.1:8090")]
    pub web_addr: String,
    #[arg(long, value_delimiter = ',', default_value = "http://localhost:3000")]
    pub cors_origin: Vec<String>,
    #[arg(long, default_value = "http://localhost:8090")]
    pub public_base_url: String,
    #[arg(long, default_value = "http://localhost:3000")]
    pub app_base_url: String,
    #[arg(long, default_value = "/")]
    pub post_login_path: String,
    #[arg(long, default_value = "/oauth/oidc/callback")]
    pub redirect_path: String,
    #[arg(long, env = "OIDC_ISSUER_URL")]
    pub issuer_url: String,
    #[arg(long, env = "OIDC_CLIENT_ID")]
    pub client_id: String,
    #[arg(long, env = "OIDC_CLIENT_SECRET")]
    pub client_secret: Option<String>,
    #[arg(long, env = "OIDC_SCOPES", default_value = "openid,profile,email")]
    pub scopes: String,
    /// Maximum lifetime of a single-use authorization request.
    #[arg(long, env = "OIDC_STATE_TTL_SECS", default_value_t = 600)]
    pub oidc_state_ttl_secs: u64,
    /// Bound unauthenticated authorization requests retained in memory.
    #[arg(long, env = "OIDC_MAX_PENDING_STATES", default_value_t = 4096)]
    pub oidc_max_pending_states: usize,
    /// Bound unauthenticated JSON requests before parsing and retaining return paths.
    #[arg(long, env = "OIDC_MAX_REQUEST_BYTES", default_value_t = 4096)]
    pub oidc_max_request_bytes: usize,
    #[arg(long, env = "AUTH_SESSION_KEY")]
    pub session_signing_key: String,
    #[arg(long, default_value_t = 3600)]
    pub session_ttl_secs: u64,
    #[arg(long, default_value_t = false)]
    pub cookie_secure: bool,
}

impl Config {
    pub fn load() -> Self {
        Config::parse()
    }
}

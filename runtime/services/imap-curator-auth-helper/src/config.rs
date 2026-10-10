use clap::{Parser, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "imap-curator-auth-helper",
    version,
    about = "Generate IMAP account connection configs and tokens"
)]
pub struct Config {
    #[arg(long, value_enum, default_value_t = Mode::Web)]
    pub mode: Mode,
    #[arg(long, default_value_t = 3600)]
    pub interval_secs: u64,
    #[arg(long, env = "IMAP_AUTH_HELPER_ADDR", default_value = "127.0.0.1:8080")]
    pub web_addr: String,
    #[arg(long, value_delimiter = ',', default_value = "http://localhost:3000")]
    pub cors_origin: Vec<String>,
    #[arg(long, default_value = "http://localhost:3000")]
    pub public_base_url: String,
    #[arg(long, env = "AUTH_SESSION_KEY")]
    pub auth_session_key: String,
    #[arg(
        long,
        env = "AUTH_SESSION_ISSUER",
        default_value = "http://localhost:8090"
    )]
    pub auth_session_issuer: String,
    #[arg(long)]
    pub database_url: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Mode {
    Once,
    Daemon,
    Web,
    #[value(name = "openapi")]
    OpenApi,
}

impl Config {
    pub fn load() -> Self {
        Config::parse()
    }
}

mod app;
mod config;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let config = config::Config::load();
    let observability = observability::Config::from_env("imap-curator-auth-helper");
    let _guard = observability::init(observability)?;
    app::run(config).await
}

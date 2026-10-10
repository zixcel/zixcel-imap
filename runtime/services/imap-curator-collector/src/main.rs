mod app;
mod config;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let config = config::Config::load();
    let mut observability = observability::Config::from_env("imap-curator-collector");
    if config.log_filter.is_some() {
        observability.log_filter = config.log_filter.clone();
    }
    let _guard = observability::init(observability)?;
    app::run(config).await
}

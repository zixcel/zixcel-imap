mod app;
mod config;
mod oidc;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let config = config::Config::load();
    let observability = observability::Config::from_env("auth");
    let _guard = observability::init(observability)?;
    app::run(config).await
}

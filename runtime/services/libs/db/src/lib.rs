use anyhow::{Context, Result};
use sqlx_postgres::PgPool;
use sqlx_postgres::PgPoolOptions;

#[derive(Debug, Clone)]
pub struct DbConfig {
    pub database_url: String,
    pub max_connections: u32,
    pub retry_attempts: u32,
    pub retry_delay_secs: u64,
}

impl DbConfig {
    /// Deployment settings are explicit; no implicit database retry/pool defaults.
    pub fn from_url_env(database_url: String) -> Result<Self> {
        Self::from_lookup(database_url, |key| std::env::var(key))
    }

    fn from_lookup(
        database_url: String,
        lookup: impl Fn(&str) -> std::result::Result<String, std::env::VarError>,
    ) -> Result<Self> {
        let positive = |key: &str| -> Result<u32> {
            let text = lookup(key).with_context(|| format!("{key} is not set"))?;
            let value = text
                .parse::<u32>()
                .with_context(|| format!("{key} must be an integer"))?;
            anyhow::ensure!(value > 0, "{key} must be positive");
            Ok(value)
        };
        Ok(Self {
            database_url,
            max_connections: positive("DATABASE_MAX_CONNECTIONS")?,
            retry_attempts: positive("DATABASE_RETRY_ATTEMPTS")?,
            retry_delay_secs: u64::from(positive("DATABASE_RETRY_DELAY_SECS")?),
        })
    }

    pub fn from_env() -> Result<Self> {
        let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is not set")?;
        Self::from_url_env(database_url)
    }
}

#[tracing::instrument(skip(config))]
pub async fn connect_pool(config: &DbConfig) -> Result<PgPool> {
    let mut last_err = None;
    for attempt in 1..=config.retry_attempts {
        match PgPoolOptions::new()
            .max_connections(config.max_connections)
            .connect(&config.database_url)
            .await
        {
            Ok(pool) => return Ok(pool),
            Err(err) => {
                last_err = Some(err);
                tracing::warn!(attempt, "database unavailable, retrying");
                tokio::time::sleep(std::time::Duration::from_secs(config.retry_delay_secs)).await;
            }
        }
    }

    let err = last_err.context("database connection failed")?;
    anyhow::bail!("database connection failed: {}", err);
}

#[tracing::instrument(skip(pool))]
pub async fn migrate(pool: &PgPool) -> Result<()> {
    sqlx_core::migrate::Migrator::new(EmbeddedMigrations)
        .await?
        .run(pool)
        .await?;
    Ok(())
}

/// Keep migrations embedded with their original bytes/checksums, without SQLx macros
/// that pull unrelated database drivers into the dependency graph.
#[derive(Debug)]
struct EmbeddedMigrations;

impl sqlx_core::migrate::MigrationSource<'static> for EmbeddedMigrations {
    fn resolve(
        self,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = std::result::Result<
                        Vec<sqlx_core::migrate::Migration>,
                        sqlx_core::error::BoxDynError,
                    >,
                > + Send
                + 'static,
        >,
    > {
        Box::pin(async {
            use sqlx_core::migrate::{Migration, MigrationType};
            use std::borrow::Cow;
            Ok(vec![
                Migration::new(
                    1,
                    Cow::Borrowed("init"),
                    MigrationType::Simple,
                    Cow::Borrowed(include_str!("../migrations/0001_init.sql")),
                    false,
                ),
                Migration::new(
                    2,
                    Cow::Borrowed("core"),
                    MigrationType::Simple,
                    Cow::Borrowed(include_str!("../migrations/0002_core.sql")),
                    false,
                ),
            ])
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx_core::migrate::MigrationSource;
    use std::{collections::BTreeMap, env::VarError};

    #[test]
    fn database_limits_are_explicit_positive_settings() {
        let mut values = BTreeMap::from([
            ("DATABASE_MAX_CONNECTIONS", "7"),
            ("DATABASE_RETRY_ATTEMPTS", "4"),
            ("DATABASE_RETRY_DELAY_SECS", "3"),
        ]);
        let make = |values: &BTreeMap<&str, &str>| {
            DbConfig::from_lookup("fixture".into(), |key| {
                values
                    .get(key)
                    .map(|s| s.to_string())
                    .ok_or(VarError::NotPresent)
            })
        };
        let config = make(&values).unwrap();
        assert_eq!(
            (
                config.max_connections,
                config.retry_attempts,
                config.retry_delay_secs
            ),
            (7, 4, 3)
        );
        for invalid in ["0", "-1", "invalid", "4294967296"] {
            values.insert("DATABASE_MAX_CONNECTIONS", invalid);
            assert!(make(&values).is_err());
        }
        values.remove("DATABASE_MAX_CONNECTIONS");
        assert!(make(&values).is_err());
    }

    #[tokio::test]
    async fn embedded_migrations_preserve_filesystem_migration_identity() {
        let embedded = EmbeddedMigrations.resolve().await.unwrap();
        let folder = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations"));
        let filesystem = folder.resolve().await.unwrap();
        assert_eq!(embedded.len(), filesystem.len());
        for (actual, previous) in embedded.iter().zip(filesystem) {
            assert_eq!(actual.version, previous.version);
            assert_eq!(actual.description, previous.description);
            assert_eq!(actual.migration_type, previous.migration_type);
            assert_eq!(actual.sql, previous.sql);
            assert_eq!(actual.checksum, previous.checksum);
            assert_eq!(actual.no_tx, previous.no_tx);
        }
    }
}

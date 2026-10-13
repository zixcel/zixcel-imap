//! IMAP operator worker that moves messages based on domain rules.

use anyhow::{Context, Result};
use checkback_core::tenant::list_tenant_ids;
use db::{DbConfig, connect_pool};
use imap_curator_common::accounts::{AccountIndexEntry, AuthConfig};
use imap_curator_common::db::{
    load_account_conn, load_accounts_index, load_domains_config, upsert_service_status,
};
use imap_curator_common::domains::AccountDomains;
use imap_curator_common::imap_client::ImapSession;
use imap_curator_common::openbao::OpenBaoClient;
use tracing::{info, warn};

use crate::config::{Config, Mode};

pub async fn run(config: Config) -> Result<()> {
    let database_url = config
        .database_url
        .clone()
        .or_else(|| std::env::var("DATABASE_URL").ok())
        .context("DATABASE_URL is not set")?;
    let pool = connect_pool(&DbConfig::from_url_env(database_url)?).await?;
    let openbao = OpenBaoClient::from_env()?;
    if let Some(openbao) = openbao.as_ref() {
        openbao.wait_ready().await?;
    }

    match config.mode {
        Mode::Once => {
            run_once(&config, &pool, openbao.as_ref()).await?;
        }
        Mode::Daemon => loop {
            if let Err(err) = run_once(&config, &pool, openbao.as_ref()).await {
                warn!(error = ?err, "operator run failed");
            }
            tokio::time::sleep(std::time::Duration::from_secs(config.interval_secs)).await;
        },
    }

    Ok(())
}

async fn run_once(
    cli: &Config,
    pool: &sqlx_postgres::PgPool,
    openbao: Option<&OpenBaoClient>,
) -> Result<()> {
    upsert_service_status(pool, "operator", cli.interval_secs, None).await?;
    let tenants = list_tenant_ids(pool).await?;
    info!(
        mode = "once",
        tenants = tenants.len(),
        "operator run started"
    );
    for tenant_id in tenants {
        match run_tenant(cli, pool, openbao, &tenant_id).await {
            Ok(()) => observability::record_job_result("operator", true),
            Err(err) => {
                observability::record_job_result("operator", false);
                return Err(err);
            }
        }
    }

    info!(mode = "once", "operator run completed");
    Ok(())
}

async fn run_tenant(
    cli: &Config,
    pool: &sqlx_postgres::PgPool,
    openbao: Option<&OpenBaoClient>,
    tenant_id: &str,
) -> Result<()> {
    let config = load_domains_config(pool, tenant_id)
        .await?
        .context("domains config missing; run collector first")?;
    let index = load_accounts_index(pool, tenant_id).await?;
    info!(
        mode = "once",
        tenant_id,
        accounts = index.accounts.len(),
        "operator tenant run started"
    );
    let accounts = index
        .accounts
        .iter()
        .filter(|entry| entry.enabled)
        .filter(|entry| {
            cli.account_id
                .as_ref()
                .is_none_or(|id| id == &entry.account_id)
        })
        .collect::<Vec<_>>();

    for entry in accounts {
        let account_cfg = match config.accounts.get(&entry.account_id) {
            Some(cfg) => cfg,
            None => {
                warn!(tenant_id, account_id = %entry.account_id, "no domains config for account");
                continue;
            }
        };
        handle_account(pool, openbao, tenant_id, entry, account_cfg, &config, cli).await?;
    }

    info!(mode = "once", tenant_id, "operator tenant run completed");
    Ok(())
}

async fn handle_account(
    pool: &sqlx_postgres::PgPool,
    openbao: Option<&OpenBaoClient>,
    tenant_id: &str,
    entry: &AccountIndexEntry,
    account_cfg: &AccountDomains,
    config: &imap_curator_common::domains::DomainsConfig,
    cli: &Config,
) -> Result<()> {
    let account_id = &entry.account_id;
    let conn_ref = &entry.conn_ref;
    let conn = load_account_conn(pool, tenant_id, conn_ref).await?;
    let secret = read_secret(openbao, &conn.auth).await?;
    let mut session = ImapSession::connect(&conn, &secret)?;
    session.select(&conn.mailboxes.inbox)?;

    let move_limit = cli.move_limit.unwrap_or(config.defaults.move_limit_per_run) as u64;
    let auto_subscribe = cli.auto_subscribe.unwrap_or(config.defaults.auto_subscribe);
    let move_batch_size = conn.limits.move_batch_size.max(1) as usize;

    let mut moved_total = 0u64;

    let mut domains = account_cfg
        .domains
        .iter()
        .filter(|(_, entry)| entry.enabled)
        .collect::<Vec<_>>();
    domains.sort_by_key(|(domain, _)| domain.to_string());

    for (domain, entry) in domains {
        if moved_total >= move_limit {
            info!(account_id, "move limit reached");
            break;
        }
        let target_folder = &entry.folder;
        session.ensure_mailbox(target_folder)?;
        if auto_subscribe {
            session.subscribe(target_folder)?;
        }

        let query = format!("HEADER From \"{}\"", domain);
        let mut uids = session.uid_search(&query)?;
        if uids.is_empty() {
            continue;
        }

        if moved_total + uids.len() as u64 > move_limit {
            let remaining = (move_limit - moved_total) as usize;
            uids.truncate(remaining);
        }

        for chunk in uids.chunks(move_batch_size) {
            if cli.dry_run {
                info!(
                    account_id,
                    domain,
                    count = chunk.len(),
                    "dry-run: skipped move"
                );
            } else {
                session.uid_move(chunk, target_folder)?;
            }
            moved_total += chunk.len() as u64;
            if moved_total >= move_limit {
                break;
            }
        }
    }

    Ok(())
}

async fn read_secret(openbao: Option<&OpenBaoClient>, auth: &AuthConfig) -> Result<String> {
    imap_curator_common::accounts::read_auth_secret(openbao, auth).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn cli_defaults_are_stable() {
        let config = Config::parse_from(["imap-curator-operator"]);
        assert!(matches!(config.mode, Mode::Daemon));
        assert_eq!(config.interval_secs, 21600);
        assert!(!config.dry_run);
        assert!(config.database_url.is_none());
    }

    #[tokio::test]
    async fn read_secret_requires_openbao_for_refs() {
        let auth = AuthConfig::Xoauth2 {
            username: "user@example.com".to_string(),
            access_token_ref: "secret/fixture/access".to_string(),
        };
        let err = read_secret(None, &auth).await.expect_err("missing openbao");
        assert!(format!("{err}").contains("OpenBao not configured"));
    }
}

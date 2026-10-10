//! IMAP domain discovery worker for Checkback IMAP Curator.

use std::collections::HashSet;

use anyhow::{Context, Result};
use checkback_core::tenant::list_tenant_ids;
use chrono::{DateTime, Utc};
use db::{DbConfig, connect_pool};
use imap_curator_common::accounts::AuthConfig;
use imap_curator_common::db::{
    load_account_conn, load_accounts_index, load_domains_config, record_account_sync_status,
    upsert_service_status, write_domains_config,
};
use imap_curator_common::domains::{
    Defaults, DomainsConfig, apply_domain_observation, ensure_account_entry,
    extract_domains_from_from_header, normalize_domain,
};
use imap_curator_common::imap_client::{HeaderSnapshot, ImapSession};
use imap_curator_common::openbao::OpenBaoClient;
use sqlx_core::query_builder::QueryBuilder;
use tracing::{debug, info};

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
        Mode::Once => run_once(&config, &pool, openbao.as_ref()).await,
        Mode::Daemon => loop {
            if let Err(err) = run_once(&config, &pool, openbao.as_ref()).await {
                tracing::warn!(error = ?err, "collector run failed");
            }
            tokio::time::sleep(std::time::Duration::from_secs(config.interval_secs)).await;
        },
    }
}

async fn run_once(
    cli: &Config,
    pool: &sqlx_postgres::PgPool,
    openbao: Option<&OpenBaoClient>,
) -> Result<()> {
    upsert_service_status(pool, "collector", cli.interval_secs, None).await?;
    let tenants = list_tenant_ids(pool).await?;
    info!(
        mode = "once",
        tenants = tenants.len(),
        "collector run started"
    );
    for tenant_id in tenants {
        match run_tenant(cli, pool, openbao, &tenant_id).await {
            Ok(()) => observability::record_job_result("collector", true),
            Err(err) => {
                observability::record_job_result("collector", false);
                return Err(err);
            }
        }
    }

    info!(mode = "once", "collector run completed");
    Ok(())
}

async fn run_tenant(
    cli: &Config,
    pool: &sqlx_postgres::PgPool,
    openbao: Option<&OpenBaoClient>,
    tenant_id: &str,
) -> Result<()> {
    let index = load_accounts_index(pool, tenant_id).await?;
    info!(
        mode = "once",
        tenant_id,
        accounts = index.accounts.len(),
        "collector tenant run started"
    );
    let mut config = match load_domains_config(pool, tenant_id).await? {
        Some(cfg) => {
            debug!(
                tenant_id,
                accounts = cfg.accounts.len(),
                "loaded domains config"
            );
            cfg
        }
        None => {
            debug!(tenant_id, "domains config missing; starting fresh");
            DomainsConfig {
                version: 1,
                defaults: Defaults::default(),
                accounts: Default::default(),
            }
        }
    };

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
    debug!(
        tenant_id,
        requested_account = ?cli.account_id,
        enabled_accounts = accounts.len(),
        "collector account selection"
    );

    for entry in accounts {
        if let Err(err) = process_account(cli, pool, openbao, tenant_id, &mut config, entry).await {
            let error_text = truncate_error(&format!("{:#}", err));
            record_account_sync_status(
                pool,
                tenant_id,
                &entry.account_id,
                "collector",
                false,
                Some(&error_text),
            )
            .await?;
            tracing::warn!(
                tenant_id,
                account_id = %entry.account_id,
                error = ?err,
                "collector account failed"
            );
        } else {
            record_account_sync_status(pool, tenant_id, &entry.account_id, "collector", true, None)
                .await?;
        }
    }

    info!(mode = "once", tenant_id, "collector tenant run completed");
    Ok(())
}

async fn process_account(
    cli: &Config,
    pool: &sqlx_postgres::PgPool,
    openbao: Option<&OpenBaoClient>,
    tenant_id: &str,
    config: &mut DomainsConfig,
    entry: &imap_curator_common::accounts::AccountIndexEntry,
) -> Result<()> {
    let conn = load_account_conn(pool, tenant_id, &entry.conn_ref).await?;
    let secret = read_secret(openbao, &conn.auth).await?;
    let mut session = ImapSession::connect(&conn, &secret)?;
    let mailbox_name = conn.mailboxes.inbox.clone();
    let mailbox = session.select(&mailbox_name)?;
    let highest_uid = mailbox.uid_next.unwrap_or(1).saturating_sub(1);

    let account_entry = ensure_account_entry(config, &conn.account_id, &mailbox_name);
    let last_uid_processed = account_entry.collector_state.last_uid_processed;
    let mut next_uid = last_uid_processed.saturating_add(1).max(1);

    if next_uid > highest_uid {
        info!(
            account_id = %conn.account_id,
            mailbox = %mailbox_name,
            next_uid,
            highest_uid,
            "collector up to date"
        );
        return Ok(());
    }

    let batch_size = cli
        .batch_size
        .unwrap_or(config.defaults.collect_batch_size)
        .max(1);
    let max_batches = cli.max_batches.unwrap_or(u32::MAX);
    let mut batches = 0u32;

    debug!(
        account_id = %conn.account_id,
        mailbox = %mailbox_name,
        last_uid_processed,
        next_uid,
        highest_uid,
        batch_size,
        max_batches,
        exclude_domains = ?config.defaults.exclude_domains,
        "collector scan config"
    );

    let exclude_set: HashSet<String> = config
        .defaults
        .exclude_domains
        .iter()
        .map(|d| normalize_domain(d))
        .collect();

    while next_uid <= highest_uid && batches < max_batches {
        let end_uid = (next_uid + batch_size - 1).min(highest_uid);
        let query = format!("UID {}:{}", next_uid, end_uid);
        let uids = session.uid_search(&query)?;
        let uid_count = uids.len();
        let headers = session.uid_fetch_headers(&uids)?;
        let header_count = headers.len();
        let now = Utc::now();
        let mut batch_domains = 0u64;
        let mut batch_unique: HashSet<String> = HashSet::new();
        let mut new_domains = 0u64;
        let mut skipped_excluded = 0u64;
        let mut skipped_empty = 0u64;
        let mut headers_without_domains = 0u64;
        let mut mail_messages = Vec::new();

        if uid_count == 0 {
            debug!(
                account_id = %conn.account_id,
                mailbox = %mailbox_name,
                uid_range = %query,
                "uid search returned no results"
            );
        }

        {
            let account_entry = ensure_account_entry(config, &conn.account_id, &mailbox_name);
            for header in headers {
                let mut domains = Vec::new();
                for from_value in &header.from {
                    domains.extend(extract_domains_from_from_header(from_value));
                }
                if domains.is_empty() {
                    headers_without_domains = headers_without_domains.saturating_add(1);
                }
                let mut primary_domain = None;
                for domain in domains.iter() {
                    let norm = normalize_domain(domain);
                    if norm.is_empty() {
                        skipped_empty = skipped_empty.saturating_add(1);
                        continue;
                    }
                    if exclude_set.contains(&norm) {
                        skipped_excluded = skipped_excluded.saturating_add(1);
                        continue;
                    }
                    if primary_domain.is_none() {
                        primary_domain = Some(norm.clone());
                    }
                    if !account_entry.domains.contains_key(&norm) {
                        new_domains = new_domains.saturating_add(1);
                    }
                    apply_domain_observation(account_entry, &norm, now);
                    batch_domains = batch_domains.saturating_add(1);
                    batch_unique.insert(norm);
                }

                mail_messages.push(build_mail_message(&header, primary_domain));
            }
        }

        let missing_headers = uid_count.saturating_sub(header_count);
        debug!(
            account_id = %conn.account_id,
            mailbox = %mailbox_name,
            batch = batches + 1,
            uid_range = %query,
            uids = uid_count,
            headers = header_count,
            missing_headers,
            domains = batch_domains,
            unique_domains = batch_unique.len(),
            new_domains,
            skipped_empty,
            skipped_excluded,
            headers_without_domains,
            last_uid_processed = end_uid,
            "collector batch processed"
        );

        {
            let account_entry = ensure_account_entry(config, &conn.account_id, &mailbox_name);
            account_entry.collector_state.last_uid_processed = end_uid;
        }
        next_uid = end_uid + 1;
        batches += 1;

        if cli.dry_run {
            info!(
                tenant_id,
                account_id = %conn.account_id,
                batch = batches,
                "dry-run: skipped domains table update"
            );
        } else {
            write_domains_config(pool, tenant_id, config).await?;
            write_mail_messages(
                pool,
                tenant_id,
                &conn.account_id,
                &mailbox_name,
                now,
                &mail_messages,
            )
            .await?;
            let domain_count = config
                .accounts
                .get(&conn.account_id)
                .map(|account| account.domains.len())
                .unwrap_or(0);
            debug!(
                tenant_id,
                account_id = %conn.account_id,
                mailbox = %mailbox_name,
                domains = domain_count,
                "domains config updated"
            );
        }
    }

    Ok(())
}

fn truncate_error(message: &str) -> String {
    if message.contains("OpenBao not configured")
        || message.contains("openbao read")
        || message.contains("openbao write")
    {
        return "Secret store unavailable. Check OpenBao service and token.".to_string();
    }
    if message.contains("AUTHENTICATIONFAILED") {
        return "Authentication failed. Verify Apple ID email, app-specific password, and IMAP access."
            .to_string();
    }
    let max = 220usize;
    if message.len() <= max {
        message.to_string()
    } else {
        format!("{}...[truncated]", &message[..max])
    }
}

struct MailMessage {
    uid: u32,
    from_address: Option<String>,
    subject: Option<String>,
    domain: Option<String>,
}

fn build_mail_message(header: &HeaderSnapshot, primary_domain: Option<String>) -> MailMessage {
    MailMessage {
        uid: header.uid,
        from_address: header.from.first().cloned(),
        subject: header.subject.clone(),
        domain: primary_domain,
    }
}

async fn write_mail_messages(
    pool: &sqlx_postgres::PgPool,
    tenant_id: &str,
    account_id: &str,
    mailbox: &str,
    collected_at: DateTime<Utc>,
    messages: &[MailMessage],
) -> Result<()> {
    if messages.is_empty() {
        return Ok(());
    }

    let mut builder = QueryBuilder::new(
        "INSERT INTO mail_messages (tenant_id, account_id, uid, mailbox, from_address, subject, domain, collected_at) ",
    );
    builder.push_values(messages, |mut row, message| {
        row.push_bind(tenant_id);
        row.push_bind(account_id);
        row.push_bind(message.uid as i64);
        row.push_bind(mailbox);
        row.push_bind(&message.from_address);
        row.push_bind(&message.subject);
        row.push_bind(&message.domain);
        row.push_bind(collected_at);
    });
    builder.push(
        " ON CONFLICT (tenant_id, account_id, uid) DO UPDATE SET \
        mailbox = EXCLUDED.mailbox, \
        from_address = EXCLUDED.from_address, \
        subject = EXCLUDED.subject, \
        domain = EXCLUDED.domain, \
        collected_at = EXCLUDED.collected_at",
    );
    builder.build().execute(pool).await?;
    Ok(())
}

async fn read_secret(openbao: Option<&OpenBaoClient>, auth: &AuthConfig) -> Result<String> {
    match auth {
        AuthConfig::Password {
            password,
            password_ref,
            ..
        } => {
            resolve_secret(
                openbao,
                password.as_deref(),
                password_ref.as_deref(),
                "password",
            )
            .await
        }
        AuthConfig::Xoauth2 {
            access_token,
            access_token_ref,
            ..
        } => {
            resolve_secret(
                openbao,
                access_token.as_deref(),
                access_token_ref.as_deref(),
                "access token",
            )
            .await
        }
    }
}

async fn resolve_secret(
    openbao: Option<&OpenBaoClient>,
    inline: Option<&str>,
    reference: Option<&str>,
    label: &str,
) -> Result<String> {
    if let Some(value) = inline {
        return Ok(value.to_string());
    }
    if let Some(reference) = reference {
        if let Some(openbao) = openbao {
            return openbao
                .read_value(reference)
                .await
                .with_context(|| format!("openbao read {}", label));
        }
        anyhow::bail!("OpenBao not configured for {}", label);
    }
    anyhow::bail!("missing {}", label);
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn cli_defaults_are_stable() {
        let config = Config::parse_from(["imap-curator-collector"]);
        assert!(matches!(config.mode, Mode::Daemon));
        assert_eq!(config.interval_secs, 21600);
        assert!(!config.dry_run);
        assert!(config.database_url.is_none());
    }

    #[tokio::test]
    async fn read_secret_prefers_inline() {
        let auth = AuthConfig::Password {
            username: "user@example.com".to_string(),
            password_ref: Some("ref".to_string()),
            password: Some("inline".to_string()),
        };
        let secret = read_secret(None, &auth).await.expect("secret");
        assert_eq!(secret, "inline");
    }

    #[tokio::test]
    async fn read_secret_requires_openbao_for_refs() {
        let auth = AuthConfig::Xoauth2 {
            username: "user@example.com".to_string(),
            access_token_ref: Some("ref".to_string()),
            access_token: None,
        };
        let err = read_secret(None, &auth).await.expect_err("missing openbao");
        assert!(format!("{err}").contains("OpenBao not configured"));
    }
}

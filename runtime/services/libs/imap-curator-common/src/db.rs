//! Database access helpers for common configuration data.

use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::Utc;
use sqlx_core::row::Row;
use sqlx_postgres::PgPool;

use crate::accounts::{
    AccountConn, AccountIndexEntry, AccountsIndex, AuthConfig, ImapConfig, Limits, Mailboxes,
    TlsConfig,
};
use crate::domains::{
    AccountDomains, CollectorState, Defaults, Discovered, DomainEntry, DomainsConfig,
};

#[tracing::instrument(skip(pool))]
pub async fn load_accounts_index(pool: &PgPool, tenant_id: &str) -> Result<AccountsIndex> {
    let rows = sqlx_core::query::query(
        "SELECT tenant_id, account_id, label, provider, enabled FROM accounts WHERE tenant_id = $1 ORDER BY account_id",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;

    let accounts = rows
        .into_iter()
        .map(|row| {
            let account_id: String = row.get("account_id");
            AccountIndexEntry {
                tenant_id: row.get("tenant_id"),
                account_id: account_id.clone(),
                label: row.get("label"),
                provider: row.get("provider"),
                enabled: row.get("enabled"),
                conn_ref: account_id,
            }
        })
        .collect();

    Ok(AccountsIndex {
        version: 1,
        generated_at: Utc::now(),
        accounts,
    })
}

#[tracing::instrument(skip(pool))]
pub async fn load_account_conn(
    pool: &PgPool,
    tenant_id: &str,
    account_id: &str,
) -> Result<AccountConn> {
    let row = sqlx_core::query::query(
        r#"
        SELECT
            a.account_id,
            a.tenant_id,
            i.host,
            i.port,
            i.tls_mode,
            i.tls_verify,
            l.search_page_size,
            l.move_batch_size,
            m.inbox,
            auth.auth_type,
            auth.username,
            auth.access_token,
            auth.access_token_ref,
            auth.app_password,
            auth.app_password_ref
        FROM accounts a
        JOIN account_imap i ON i.account_id = a.account_id AND i.tenant_id = a.tenant_id
        JOIN account_limits l ON l.account_id = a.account_id AND l.tenant_id = a.tenant_id
        JOIN account_mailboxes m ON m.account_id = a.account_id AND m.tenant_id = a.tenant_id
        JOIN account_auth auth ON auth.account_id = a.account_id AND auth.tenant_id = a.tenant_id
        WHERE a.tenant_id = $1 AND a.account_id = $2
        "#,
    )
    .bind(tenant_id)
    .bind(account_id)
    .fetch_one(pool)
    .await?;

    let auth_type: String = row.get("auth_type");
    let username: String = row.get("username");
    let auth = match auth_type.as_str() {
        "xoauth2" => AuthConfig::Xoauth2 {
            username,
            access_token_ref: row.try_get("access_token_ref")?,
            access_token: row.try_get("access_token")?,
        },
        "password" => AuthConfig::Password {
            username,
            password_ref: row.try_get("app_password_ref")?,
            password: row.try_get("app_password")?,
        },
        other => anyhow::bail!("unsupported auth_type: {}", other),
    };

    Ok(AccountConn {
        version: 1,
        tenant_id: row.get("tenant_id"),
        account_id: row.get("account_id"),
        imap: ImapConfig {
            host: row.get("host"),
            port: row.get::<i32, _>("port") as u16,
            tls: TlsConfig {
                mode: row.get("tls_mode"),
                verify_cert: row.get("tls_verify"),
            },
        },
        auth,
        mailboxes: Mailboxes {
            inbox: row.get("inbox"),
        },
        limits: Limits {
            search_page_size: row.get::<i32, _>("search_page_size") as u32,
            move_batch_size: row.get::<i32, _>("move_batch_size") as u32,
        },
    })
}

#[tracing::instrument(skip(pool))]
pub async fn load_domains_config(pool: &PgPool, tenant_id: &str) -> Result<Option<DomainsConfig>> {
    let defaults_row = sqlx_core::query::query(
        "SELECT auto_subscribe, move_limit_per_run, collect_batch_size, exclude_domains FROM domain_defaults WHERE tenant_id = $1 AND id = 1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await?;

    let defaults = if let Some(row) = defaults_row {
        Defaults {
            auto_subscribe: row.get("auto_subscribe"),
            move_limit_per_run: row.get::<i32, _>("move_limit_per_run") as u32,
            collect_batch_size: row.get::<i32, _>("collect_batch_size") as u32,
            exclude_domains: row.try_get("exclude_domains").unwrap_or_default(),
        }
    } else {
        Defaults::default()
    };

    let account_rows = sqlx_core::query::query(
        r#"
        SELECT
            a.account_id,
            COALESCE(cs.mailbox, m.inbox) AS mailbox,
            COALESCE(cs.last_uid_processed, 0) AS last_uid_processed
        FROM accounts a
        LEFT JOIN account_mailboxes m ON m.account_id = a.account_id AND m.tenant_id = a.tenant_id
        LEFT JOIN collector_state cs ON cs.account_id = a.account_id AND cs.tenant_id = a.tenant_id
        WHERE a.tenant_id = $1
        "#,
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;

    let mut accounts: HashMap<String, AccountDomains> = HashMap::new();
    for row in account_rows {
        let account_id: String = row.get("account_id");
        accounts.insert(
            account_id,
            AccountDomains {
                mailbox: row.get("mailbox"),
                collector_state: CollectorState {
                    last_uid_processed: row.get::<i32, _>("last_uid_processed") as u32,
                },
                domains: HashMap::new(),
            },
        );
    }

    let domain_rows = sqlx_core::query::query(
        "SELECT account_id, domain, enabled, folder, first_seen, last_seen, count_estimate FROM account_domains WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;

    for row in domain_rows {
        let account_id: String = row.get("account_id");
        let entry = accounts
            .entry(account_id)
            .or_insert_with(|| AccountDomains {
                mailbox: "INBOX".to_string(),
                collector_state: CollectorState::default(),
                domains: HashMap::new(),
            });
        let domain: String = row.get("domain");
        entry.domains.insert(
            domain,
            DomainEntry {
                enabled: row.get("enabled"),
                folder: row.get("folder"),
                discovered: Discovered {
                    first_seen: row.get("first_seen"),
                    last_seen: row.get("last_seen"),
                    count_estimate: row.get::<i64, _>("count_estimate") as u64,
                },
            },
        );
    }

    Ok(Some(DomainsConfig {
        version: 1,
        defaults,
        accounts,
    }))
}

#[tracing::instrument(skip(pool, config))]
pub async fn write_domains_config(
    pool: &PgPool,
    tenant_id: &str,
    config: &DomainsConfig,
) -> Result<()> {
    let mut tx = pool.begin().await?;

    sqlx_core::query::query(
        r#"
        INSERT INTO domain_defaults (tenant_id, id, auto_subscribe, move_limit_per_run, collect_batch_size, exclude_domains)
        VALUES ($1, 1, $2, $3, $4, $5)
        ON CONFLICT (tenant_id, id) DO UPDATE SET
            auto_subscribe = EXCLUDED.auto_subscribe,
            move_limit_per_run = EXCLUDED.move_limit_per_run,
            collect_batch_size = EXCLUDED.collect_batch_size,
            exclude_domains = EXCLUDED.exclude_domains
        "#,
    )
    .bind(tenant_id)
    .bind(config.defaults.auto_subscribe)
    .bind(config.defaults.move_limit_per_run as i32)
    .bind(config.defaults.collect_batch_size as i32)
    .bind(&config.defaults.exclude_domains)
    .execute(&mut *tx)
    .await?;

    for (account_id, account) in &config.accounts {
        sqlx_core::query::query(
            r#"
            INSERT INTO collector_state (tenant_id, account_id, mailbox, last_uid_processed)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (tenant_id, account_id) DO UPDATE SET
                mailbox = EXCLUDED.mailbox,
                last_uid_processed = EXCLUDED.last_uid_processed
            "#,
        )
        .bind(tenant_id)
        .bind(account_id)
        .bind(&account.mailbox)
        .bind(account.collector_state.last_uid_processed as i32)
        .execute(&mut *tx)
        .await?;

        for (domain, entry) in &account.domains {
            sqlx_core::query::query(
                r#"
                INSERT INTO account_domains
                    (tenant_id, account_id, domain, enabled, folder, first_seen, last_seen, count_estimate)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                ON CONFLICT (tenant_id, account_id, domain) DO UPDATE SET
                    enabled = EXCLUDED.enabled,
                    folder = EXCLUDED.folder,
                    first_seen = EXCLUDED.first_seen,
                    last_seen = EXCLUDED.last_seen,
                    count_estimate = EXCLUDED.count_estimate
                "#,
            )
            .bind(tenant_id)
            .bind(account_id)
            .bind(domain)
            .bind(entry.enabled)
            .bind(&entry.folder)
            .bind(entry.discovered.first_seen)
            .bind(entry.discovered.last_seen)
            .bind(entry.discovered.count_estimate as i64)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await.context("commit domains config")?;
    Ok(())
}

#[tracing::instrument(skip(pool, detail))]
pub async fn upsert_service_status(
    pool: &PgPool,
    service: &str,
    interval_secs: u64,
    detail: Option<&str>,
) -> Result<()> {
    sqlx_core::query::query(
        r#"
        INSERT INTO service_status (service, last_seen, interval_secs, detail)
        VALUES ($1, now(), $2, $3)
        ON CONFLICT (service) DO UPDATE SET
            last_seen = EXCLUDED.last_seen,
            interval_secs = EXCLUDED.interval_secs,
            detail = EXCLUDED.detail
        "#,
    )
    .bind(service)
    .bind(interval_secs as i32)
    .bind(detail)
    .execute(pool)
    .await?;
    Ok(())
}

#[tracing::instrument(skip(pool, error))]
pub async fn record_account_sync_status(
    pool: &PgPool,
    tenant_id: &str,
    account_id: &str,
    service: &str,
    ok: bool,
    error: Option<&str>,
) -> Result<()> {
    let now = Utc::now();
    if ok {
        sqlx_core::query::query(
            r#"
            INSERT INTO account_sync_status
                (tenant_id, account_id, service, last_run_at, last_success_at, last_error_at, last_error)
            VALUES ($1, $2, $3, $4, $4, NULL, NULL)
            ON CONFLICT (tenant_id, account_id, service) DO UPDATE SET
                last_run_at = EXCLUDED.last_run_at,
                last_success_at = EXCLUDED.last_success_at,
                last_error_at = NULL,
                last_error = NULL
            "#,
        )
        .bind(tenant_id)
        .bind(account_id)
        .bind(service)
        .bind(now)
        .execute(pool)
        .await?;
    } else {
        sqlx_core::query::query(
            r#"
            INSERT INTO account_sync_status
                (tenant_id, account_id, service, last_run_at, last_success_at, last_error_at, last_error)
            VALUES ($1, $2, $3, $4, NULL, $4, $5)
            ON CONFLICT (tenant_id, account_id, service) DO UPDATE SET
                last_run_at = EXCLUDED.last_run_at,
                last_error_at = EXCLUDED.last_error_at,
                last_error = EXCLUDED.last_error,
                last_success_at = account_sync_status.last_success_at
            "#,
        )
        .bind(tenant_id)
        .bind(account_id)
        .bind(service)
        .bind(now)
        .bind(error)
        .execute(pool)
        .await?;
    }

    Ok(())
}

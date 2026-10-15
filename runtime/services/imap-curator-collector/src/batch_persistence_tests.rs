//! Explicit acceptance against an isolated schema in the disposable CI PostgreSQL database.
use super::*;
use sqlx_core::{query::query, query_scalar::query_scalar};
use sqlx_postgres::{PgPool, PgPoolOptions, Postgres};

#[tokio::test]
#[ignore = "requires the explicitly selected disposable PostgreSQL database"]
async fn batch_persistence_contract() -> Result<()> {
    let database_url = std::env::var("IMAP_TEST_DATABASE_URL")?;
    let parsed = url::Url::parse(&database_url)?;
    anyhow::ensure!(
        parsed.scheme() == "postgresql"
            && parsed.host_str() == Some("127.0.0.1")
            && parsed.username() == "postgres"
            && parsed.password().is_none()
            && parsed.path() == "/wonderland_test_imap"
            && parsed.query().is_none()
            && parsed.fragment().is_none(),
        "only disposable loopback PostgreSQL is allowed"
    );
    let schema = format!(
        "collector_atomic_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    );
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await?;
    query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await?;
    let path = schema.clone();
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .after_connect(move |connection, _| {
            let path = path.clone();
            Box::pin(async move {
                query("SELECT set_config('search_path', $1, false)")
                    .bind(path)
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&database_url)
        .await?;
    db::migrate(&pool).await?;
    sqlx_core::raw_sql::raw_sql(include_str!("../../libs/db/fixtures/development.sql"))
        .execute(&pool)
        .await?;
    let now = Utc::now();
    let mut initial = load_domains_config(&pool, "fixture-a")
        .await?
        .context("fixture config")?;
    ensure_account_entry(&mut initial, "fixture-account", "INBOX")
        .collector_state
        .last_uid_processed = 1;
    persist_batch(
        &pool,
        "fixture-a",
        "fixture-account",
        "INBOX",
        now,
        &initial,
        &[],
    )
    .await?;
    let mut planned = initial.clone();
    let account = ensure_account_entry(&mut planned, "fixture-account", "INBOX");
    account.collector_state.last_uid_processed = 6;
    for _ in 0..2 {
        apply_domain_observation(account, "accepted.invalid", now);
    }
    let messages: Vec<_> = [3, 5, 6]
        .into_iter()
        .map(|uid| MailMessage {
            uid,
            from_address: Some("sender@fixture.invalid".into()),
            subject: None,
            domain: if uid == 6 {
                None
            } else {
                Some("accepted.invalid".into())
            },
        })
        .collect();
    // A failure during mail insertion must roll back the previously written cursor/domains.
    sqlx_core::raw_sql::raw_sql(
        "CREATE FUNCTION fail_mail() RETURNS trigger LANGUAGE plpgsql AS $$
        BEGIN IF NEW.uid = 5 THEN RAISE EXCEPTION 'synthetic mail failure'; END IF;
        RETURN NEW; END $$; CREATE TRIGGER fail_mail BEFORE INSERT ON mail_messages
        FOR EACH ROW EXECUTE FUNCTION fail_mail();",
    )
    .execute(&pool)
    .await?;
    assert!(
        persist_batch(
            &pool,
            "fixture-a",
            "fixture-account",
            "INBOX",
            now,
            &planned,
            &messages
        )
        .await
        .is_err()
    );
    assert_state(&pool, 1, 1, 0).await?;
    sqlx_core::raw_sql::raw_sql(
        "DROP TRIGGER fail_mail ON mail_messages; DROP FUNCTION fail_mail();",
    )
    .execute(&pool)
    .await?;
    // Simulate process termination before commit: dropping the transaction rolls back everything.
    {
        let mut tx = pool.begin().await?;
        write_domains_config_in_transaction(&mut tx, "fixture-a", &planned).await?;
        write_mail_messages(
            &mut tx,
            "fixture-a",
            "fixture-account",
            "INBOX",
            now,
            &messages,
        )
        .await?;
    }
    assert_state(&pool, 1, 1, 0).await?;
    // Commit succeeds but its acknowledgement is lost: resume from durable state, not memory.
    persist_batch(
        &pool,
        "fixture-a",
        "fixture-account",
        "INBOX",
        now,
        &planned,
        &messages,
    )
    .await?;
    let mut restarted = load_domains_config(&pool, "fixture-a")
        .await?
        .context("restart")?;
    assert_eq!(
        restarted.accounts["fixture-account"]
            .collector_state
            .last_uid_processed
            + 1,
        7
    );
    assert_state(&pool, 6, 4, 2).await?;
    // Replaying the same persisted observations is an upsert, not a second count increment.
    persist_batch(
        &pool,
        "fixture-a",
        "fixture-account",
        "INBOX",
        now,
        &restarted,
        &messages,
    )
    .await?;
    assert_state(&pool, 6, 4, 2).await?;
    // Missing UIDs and intentionally excluded domains do not prevent committing a scanned range.
    restarted
        .accounts
        .get_mut("fixture-account")
        .context("account")?
        .collector_state
        .last_uid_processed = 9;
    persist_batch(
        &pool,
        "fixture-a",
        "fixture-account",
        "INBOX",
        now,
        &restarted,
        &[],
    )
    .await?;
    assert_state(&pool, 9, 4, 2).await?;
    let other: i64 = query_scalar::<Postgres, i64>(
        "SELECT count(*) FROM collector_state WHERE tenant_id='fixture-b'",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(other, 0);
    pool.close().await;
    query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await?;
    admin.close().await;
    Ok(())
}

async fn assert_state(pool: &PgPool, cursor: u32, mails: i64, observations: u64) -> Result<()> {
    let config = load_domains_config(pool, "fixture-a")
        .await?
        .context("durable config")?;
    assert_eq!(
        config.accounts["fixture-account"]
            .collector_state
            .last_uid_processed,
        cursor
    );
    assert_eq!(
        config.accounts["fixture-account"]
            .domains
            .get("accepted.invalid")
            .map_or(0, |entry| entry.discovered.count_estimate),
        observations
    );
    let count: i64 = query_scalar::<Postgres, i64>(
        "SELECT count(*) FROM mail_messages WHERE tenant_id='fixture-a'",
    )
    .fetch_one(pool)
    .await?;
    assert_eq!(count, mails);
    Ok(())
}

//! Verify embedded migrations and tenant foreign keys against a disposable local database.
use anyhow::{Result, ensure};
use sqlx_core::query::query;
use sqlx_core::query_scalar::query_scalar;
use sqlx_postgres::Postgres;

#[tokio::main]
async fn main() -> Result<()> {
    let database_url = std::env::var("IMAP_TEST_DATABASE_URL")?;
    let url = url::Url::parse(&database_url)?;
    ensure!(
        url.scheme() == "postgresql"
            && url.host_str() == Some("127.0.0.1")
            && url.username() == "postgres"
            && url.password().is_none()
            && url.path() == "/wonderland_test_imap"
            && url.query().is_none()
            && url.fragment().is_none(),
        "acceptance verification requires the disposable loopback test database"
    );
    let pool = db::connect_pool(&db::DbConfig {
        database_url,
        max_connections: 2,
        retry_attempts: 1,
        retry_delay_secs: 1,
    })
    .await?;
    db::migrate(&pool).await?;
    db::migrate(&pool).await?;
    let count: i64 =
        query_scalar::<Postgres, i64>("SELECT count(*) FROM _sqlx_migrations WHERE success")
            .fetch_one(&pool)
            .await?;
    ensure!(
        count == 1,
        "the fresh development baseline must be applied exactly once"
    );
    for _ in 0..2 {
        let mut transaction = pool.begin().await?;
        sqlx_core::raw_sql::raw_sql(include_str!("../fixtures/development.sql"))
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
    }
    let count: i64 = query_scalar::<Postgres, i64>(
        "SELECT count(*) FROM accounts WHERE tenant_id IN ('fixture-a','fixture-b')",
    )
    .fetch_one(&pool)
    .await?;
    ensure!(
        count == 2,
        "synthetic fixture registration must be idempotent"
    );
    let count: i64 = query_scalar::<Postgres, i64>(
        "SELECT count(*) FROM information_schema.columns WHERE table_schema='public'
         AND table_name='account_auth' AND column_name IN
         ('client_secret','refresh_token','app_password','access_token')",
    )
    .fetch_one(&pool)
    .await?;
    ensure!(
        count == 0,
        "plaintext credential storage columns must be absent"
    );
    query("INSERT INTO accounts (tenant_id,account_id,label,provider) VALUES ('fixture-a','fixture-default','Fixture','fixture')")
        .execute(&pool).await?;
    let enabled: bool = query_scalar::<Postgres, bool>(
        "SELECT enabled FROM accounts WHERE tenant_id='fixture-a' AND account_id='fixture-default'",
    )
    .fetch_one(&pool)
    .await?;
    ensure!(
        !enabled,
        "accounts must be disabled unless explicitly enabled"
    );
    for sql in [
        "UPDATE account_imap SET port=0 WHERE tenant_id='fixture-a'",
        "UPDATE account_imap SET port=65536 WHERE tenant_id='fixture-a'",
        "UPDATE account_imap SET tls_verify=false WHERE tenant_id='fixture-a'",
        "UPDATE account_imap SET tls_mode='plain' WHERE tenant_id='fixture-a'",
        "UPDATE account_limits SET move_batch_size=0 WHERE tenant_id='fixture-a'",
        "UPDATE account_auth SET app_password_ref=NULL WHERE tenant_id='fixture-a'",
    ] {
        expect_database_code(&pool, sql, "23514").await?;
    }
    expect_database_code(&pool,
        "INSERT INTO accounts (account_id,label,provider) VALUES ('missing-tenant','Fixture','fixture')",
        "23502").await?;
    let wrong_tenant = query("INSERT INTO account_imap (tenant_id,account_id,host,port) VALUES ('fixture-b','fixture-account','example.invalid',993)")
        .execute(&pool).await;
    ensure!(wrong_tenant.is_err_and(|error| matches!(error, sqlx_core::error::Error::Database(ref e) if e.code().as_deref() == Some("23503"))),
        "cross-tenant foreign-key references must fail");
    query("DELETE FROM accounts WHERE tenant_id='fixture-a' AND account_id='fixture-account'")
        .execute(&pool)
        .await?;
    let count: i64 = query_scalar::<Postgres, i64>(
        "SELECT count(*) FROM account_imap WHERE account_id='fixture-account'",
    )
    .fetch_one(&pool)
    .await?;
    ensure!(count == 0, "tenant-scoped dependent records must cascade");
    query("DELETE FROM tenants WHERE tenant_id IN ('fixture-a','fixture-b')")
        .execute(&pool)
        .await?;
    pool.close().await;
    println!(
        "Fresh PostgreSQL baseline, synthetic fixtures, reference-only credentials, tenant and TLS checks passed."
    );
    Ok(())
}

async fn expect_database_code(
    pool: &sqlx_postgres::PgPool,
    sql: &'static str,
    code: &str,
) -> Result<()> {
    let result = query(sql).execute(pool).await;
    ensure!(
        result.is_err_and(|error| matches!(error,
        sqlx_core::error::Error::Database(ref e) if e.code().as_deref() == Some(code))),
        "unsafe fixture statement must fail with the expected database constraint"
    );
    Ok(())
}

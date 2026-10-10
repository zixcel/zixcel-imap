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
        count == 2,
        "both original migrations must be applied exactly once"
    );
    let mut transaction = pool.begin().await?;
    query("INSERT INTO tenants (tenant_id,name) VALUES ('fixture-a','Fixture A'),('fixture-b','Fixture B')")
        .execute(&mut *transaction).await?;
    query("INSERT INTO accounts (tenant_id,account_id,label,provider) VALUES ('fixture-a','fixture-account','Fixture','fixture')")
        .execute(&mut *transaction).await?;
    query("INSERT INTO account_imap (tenant_id,account_id,host,port) VALUES ('fixture-a','fixture-account','example.invalid',993)")
        .execute(&mut *transaction).await?;
    transaction.commit().await?;
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
    println!("Disposable PostgreSQL migration and tenant foreign-key checks passed.");
    Ok(())
}

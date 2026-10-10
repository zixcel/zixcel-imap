use anyhow::{Context, Result};
use sqlx_core::row::Row;
use sqlx_postgres::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Tenant {
    pub tenant_id: String,
    pub name: String,
}

pub async fn ensure_subject(
    pool: &PgPool,
    subject_id: &str,
    email: Option<&str>,
    name: Option<&str>,
) -> Result<()> {
    sqlx_core::query::query(
        r#"
        INSERT INTO subjects (subject_id, email, name)
        VALUES ($1, $2, $3)
        ON CONFLICT (subject_id) DO UPDATE SET
            email = COALESCE(EXCLUDED.email, subjects.email),
            name = COALESCE(EXCLUDED.name, subjects.name),
            updated_at = now()
        "#,
    )
    .bind(subject_id)
    .bind(email)
    .bind(name)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn resolve_tenant_for_subject(
    pool: &PgPool,
    subject_id: &str,
    display_name: Option<&str>,
) -> Result<Tenant> {
    if let Some(tenant) = lookup_tenant_for_subject(pool, subject_id).await? {
        return Ok(tenant);
    }

    let tenant_id = Uuid::new_v4().to_string();
    let label = display_name
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or("Personal");
    let tenant_name = format!("{} workspace", label);

    let mut tx = pool.begin().await?;
    sqlx_core::query::query("INSERT INTO tenants (tenant_id, name) VALUES ($1, $2)")
        .bind(&tenant_id)
        .bind(&tenant_name)
        .execute(&mut *tx)
        .await?;
    sqlx_core::query::query(
        "INSERT INTO tenant_memberships (tenant_id, subject_id, role) VALUES ($1, $2, 'owner')",
    )
    .bind(&tenant_id)
    .bind(subject_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(Tenant {
        tenant_id,
        name: tenant_name,
    })
}

pub async fn list_tenant_ids(pool: &PgPool) -> Result<Vec<String>> {
    let rows = sqlx_core::query::query("SELECT tenant_id FROM tenants ORDER BY tenant_id")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|row| row.get("tenant_id")).collect())
}

async fn lookup_tenant_for_subject(pool: &PgPool, subject_id: &str) -> Result<Option<Tenant>> {
    let row = sqlx_core::query::query(
        r#"
        SELECT t.tenant_id, t.name
        FROM tenant_memberships m
        JOIN tenants t ON t.tenant_id = m.tenant_id
        WHERE m.subject_id = $1
        ORDER BY m.created_at
        LIMIT 1
        "#,
    )
    .bind(subject_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| Tenant {
        tenant_id: row.get("tenant_id"),
        name: row.get("name"),
    }))
}

pub async fn ensure_default_tenant(pool: &PgPool) -> Result<()> {
    sqlx_core::query::query("INSERT INTO tenants (tenant_id, name) VALUES ('default', 'Default') ON CONFLICT DO NOTHING")
        .execute(pool)
        .await
        .context("ensure default tenant")?;
    Ok(())
}

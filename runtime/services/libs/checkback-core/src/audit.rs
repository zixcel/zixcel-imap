use anyhow::Result;
use serde_json::Value;
use sqlx_core::executor::Executor;
use sqlx_postgres::Postgres;

#[derive(Debug, Clone)]
pub struct AuditRecord {
    pub tenant_id: Option<String>,
    pub subject_id: Option<String>,
    pub subject_email: Option<String>,
    pub action: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub success: bool,
    pub error_code: Option<String>,
    pub detail: Option<Value>,
}

impl AuditRecord {
    pub fn success(
        tenant_id: impl Into<String>,
        subject_id: impl Into<String>,
        subject_email: Option<String>,
        action: impl Into<String>,
    ) -> Self {
        Self {
            tenant_id: Some(tenant_id.into()),
            subject_id: Some(subject_id.into()),
            subject_email,
            action: action.into(),
            resource_type: None,
            resource_id: None,
            success: true,
            error_code: None,
            detail: None,
        }
    }

    pub fn failure(action: impl Into<String>, error_code: impl Into<String>) -> Self {
        Self {
            tenant_id: None,
            subject_id: None,
            subject_email: None,
            action: action.into(),
            resource_type: None,
            resource_id: None,
            success: false,
            error_code: Some(error_code.into()),
            detail: None,
        }
    }
}

pub async fn write_audit_log<'e, E>(executor: E, record: &AuditRecord) -> Result<()>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx_core::query::query(
        r#"
        INSERT INTO audit_log
            (tenant_id, subject_id, subject_email, action, resource_type, resource_id, success, error_code, detail)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
    )
    .bind(&record.tenant_id)
    .bind(&record.subject_id)
    .bind(&record.subject_email)
    .bind(&record.action)
    .bind(&record.resource_type)
    .bind(&record.resource_id)
    .bind(record.success)
    .bind(&record.error_code)
    .bind(&record.detail)
    .execute(executor)
    .await?;
    Ok(())
}

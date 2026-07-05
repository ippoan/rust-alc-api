use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{CreateTroubleTaskStatus, TroubleTaskStatus, UpdateTroubleTaskStatus};
use alc_core::tenant::TenantConn;

pub use crate::repository::trouble_task_statuses::*;

pub struct PgTroubleTaskStatusesRepository {
    pool: PgPool,
}

impl PgTroubleTaskStatusesRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn derive_key(key: Option<&str>, name: &str) -> String {
    if let Some(k) = key {
        let trimmed = k.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    // simple slug fallback: use the name as-is (tenants often use Japanese names).
    // The unique constraint is on (tenant_id, key) so using the name as the key is safe.
    name.trim().to_string()
}

#[async_trait]
impl TroubleTaskStatusesRepository for PgTroubleTaskStatusesRepository {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<TroubleTaskStatus>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleTaskStatus>(
            "SELECT * FROM trouble_task_statuses WHERE tenant_id = $1 ORDER BY sort_order, name",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateTroubleTaskStatus,
    ) -> Result<TroubleTaskStatus, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let key = derive_key(input.key.as_deref(), &input.name);
        let color = input.color.clone().unwrap_or_else(|| "#9CA3AF".to_string());
        sqlx::query_as::<_, TroubleTaskStatus>(
            r#"INSERT INTO trouble_task_statuses (tenant_id, key, name, color, sort_order, is_done)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *"#,
        )
        .bind(tenant_id)
        .bind(&key)
        .bind(&input.name)
        .bind(&color)
        .bind(input.sort_order.unwrap_or(0))
        .bind(input.is_done.unwrap_or(false))
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateTroubleTaskStatus,
    ) -> Result<Option<TroubleTaskStatus>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleTaskStatus>(
            r#"UPDATE trouble_task_statuses
            SET name = COALESCE($3, name),
                color = COALESCE($4, color),
                sort_order = COALESCE($5, sort_order),
                is_done = COALESCE($6, is_done),
                updated_at = now()
            WHERE id = $1 AND tenant_id = $2
            RETURNING *"#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(input.name.as_ref())
        .bind(input.color.as_ref())
        .bind(input.sort_order)
        .bind(input.is_done)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result =
            sqlx::query("DELETE FROM trouble_task_statuses WHERE id = $1 AND tenant_id = $2")
                .bind(id)
                .bind(tenant_id)
                .execute(&mut *tc.conn)
                .await?;
        Ok(result.rows_affected() > 0)
    }
}

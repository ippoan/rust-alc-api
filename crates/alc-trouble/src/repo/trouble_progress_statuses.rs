use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::{CreateTroubleProgressStatus, TroubleProgressStatus};
use alc_core::tenant::TenantConn;

pub use alc_core::repository::trouble_progress_statuses::*;

pub struct PgTroubleProgressStatusesRepository {
    pool: PgPool,
}

impl PgTroubleProgressStatusesRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TroubleProgressStatusesRepository for PgTroubleProgressStatusesRepository {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<TroubleProgressStatus>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleProgressStatus>(
            "SELECT * FROM trouble_progress_statuses WHERE tenant_id = $1 ORDER BY sort_order, name",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateTroubleProgressStatus,
    ) -> Result<TroubleProgressStatus, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleProgressStatus>(
            r#"INSERT INTO trouble_progress_statuses (tenant_id, name, sort_order)
            VALUES ($1, $2, $3)
            RETURNING *"#,
        )
        .bind(tenant_id)
        .bind(&input.name)
        .bind(input.sort_order.unwrap_or(0))
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result =
            sqlx::query("DELETE FROM trouble_progress_statuses WHERE id = $1 AND tenant_id = $2")
                .bind(id)
                .bind(tenant_id)
                .execute(&mut *tc.conn)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn update_sort_order(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        sort_order: i32,
    ) -> Result<Option<TroubleProgressStatus>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleProgressStatus>(
            "UPDATE trouble_progress_statuses SET sort_order = $3 WHERE id = $1 AND tenant_id = $2 RETURNING *",
        )
        .bind(id)
        .bind(tenant_id)
        .bind(sort_order)
        .fetch_optional(&mut *tc.conn)
        .await
    }
}

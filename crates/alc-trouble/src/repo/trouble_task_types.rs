use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::{CreateTroubleCategory, TroubleCategory};
use alc_core::tenant::TenantConn;

pub use alc_core::repository::trouble_task_types::*;

pub struct PgTroubleTaskTypesRepository {
    pool: PgPool,
}

impl PgTroubleTaskTypesRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TroubleTaskTypesRepository for PgTroubleTaskTypesRepository {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<TroubleCategory>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleCategory>(
            "SELECT * FROM trouble_task_types WHERE tenant_id = $1 ORDER BY sort_order, name",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateTroubleCategory,
    ) -> Result<TroubleCategory, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleCategory>(
            r#"INSERT INTO trouble_task_types (tenant_id, name, sort_order)
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
        let result = sqlx::query("DELETE FROM trouble_task_types WHERE id = $1 AND tenant_id = $2")
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
    ) -> Result<Option<TroubleCategory>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleCategory>(
            "UPDATE trouble_task_types SET sort_order = $3 WHERE id = $1 AND tenant_id = $2 RETURNING *",
        )
        .bind(id)
        .bind(tenant_id)
        .bind(sort_order)
        .fetch_optional(&mut *tc.conn)
        .await
    }
}

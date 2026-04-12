use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::{CreateTroubleTaskActivity, TroubleTaskActivity, UpdateTroubleTaskActivity};
use alc_core::tenant::TenantConn;

pub use alc_core::repository::trouble_task_activities::*;

pub struct PgTroubleTaskActivitiesRepository {
    pool: PgPool,
}

impl PgTroubleTaskActivitiesRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TroubleTaskActivitiesRepository for PgTroubleTaskActivitiesRepository {
    async fn create(
        &self,
        tenant_id: Uuid,
        task_id: Uuid,
        created_by: Option<Uuid>,
        input: &CreateTroubleTaskActivity,
    ) -> Result<TroubleTaskActivity, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleTaskActivity>(
            r#"INSERT INTO trouble_task_activities (tenant_id, task_id, body, occurred_at, created_by)
            VALUES ($1, $2, $3, COALESCE($4, now()), $5)
            RETURNING *"#,
        )
        .bind(tenant_id)
        .bind(task_id)
        .bind(&input.body)
        .bind(input.occurred_at)
        .bind(created_by)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn list_by_task(
        &self,
        tenant_id: Uuid,
        task_id: Uuid,
    ) -> Result<Vec<TroubleTaskActivity>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleTaskActivity>(
            "SELECT * FROM trouble_task_activities WHERE task_id = $1 AND tenant_id = $2 ORDER BY occurred_at",
        )
        .bind(task_id)
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateTroubleTaskActivity,
    ) -> Result<Option<TroubleTaskActivity>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleTaskActivity>(
            r#"UPDATE trouble_task_activities SET
                body = COALESCE($3, body),
                occurred_at = CASE WHEN $4 THEN $5 ELSE occurred_at END
            WHERE id = $1 AND tenant_id = $2
            RETURNING *"#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(&input.body)
        .bind(input.occurred_at.is_some())
        .bind(input.occurred_at.flatten())
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result =
            sqlx::query("DELETE FROM trouble_task_activities WHERE id = $1 AND tenant_id = $2")
                .bind(id)
                .bind(tenant_id)
                .execute(&mut *tc.conn)
                .await?;
        Ok(result.rows_affected() > 0)
    }
}

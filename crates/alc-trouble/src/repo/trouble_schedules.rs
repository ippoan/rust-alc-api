use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::{CreateTroubleSchedule, TroubleSchedule};
use alc_core::tenant::TenantConn;

pub use alc_core::repository::trouble_schedules::*;

pub struct PgTroubleSchedulesRepository {
    pool: PgPool,
}

impl PgTroubleSchedulesRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TroubleSchedulesRepository for PgTroubleSchedulesRepository {
    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateTroubleSchedule,
        created_by: Option<Uuid>,
    ) -> Result<TroubleSchedule, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleSchedule>(
            r#"
            INSERT INTO trouble_schedules
                (tenant_id, ticket_id, task_id, scheduled_at, message, lineworks_user_ids, created_by)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(input.ticket_id)
        .bind(input.task_id)
        .bind(input.scheduled_at)
        .bind(&input.message)
        .bind(&input.lineworks_user_ids)
        .bind(created_by)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn list_by_ticket(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
    ) -> Result<Vec<TroubleSchedule>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleSchedule>(
            r#"
            SELECT * FROM trouble_schedules
            WHERE tenant_id = $1 AND ticket_id = $2
            ORDER BY scheduled_at
            "#,
        )
        .bind(tenant_id)
        .bind(ticket_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<TroubleSchedule>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleSchedule>(
            "SELECT * FROM trouble_schedules WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn update_status(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        status: &str,
    ) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query(
            "UPDATE trouble_schedules SET status = $3 WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .bind(status)
        .execute(&mut *tc.conn)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn set_cloud_task_name(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        task_name: &str,
    ) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query(
            "UPDATE trouble_schedules SET cloud_task_name = $3 WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .bind(task_name)
        .execute(&mut *tc.conn)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn cancel_pending_by_task(
        &self,
        tenant_id: Uuid,
        task_id: Uuid,
    ) -> Result<Vec<TroubleSchedule>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleSchedule>(
            r#"
            UPDATE trouble_schedules
            SET status = 'cancelled'
            WHERE tenant_id = $1 AND task_id = $2 AND status = 'pending'
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(task_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get_for_fire(&self, id: Uuid) -> Result<Option<TroubleSchedule>, sqlx::Error> {
        // SECURITY DEFINER 関数経由でRLSバイパス
        sqlx::query_as::<_, TroubleSchedule>("SELECT * FROM get_trouble_schedule($1)")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    async fn mark_sent(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        // fire は RLS 外から呼ばれるため pool 直接使用
        let result = sqlx::query(
            "UPDATE trouble_schedules SET status = 'sent', sent_at = NOW() WHERE id = $1 AND status = 'pending'",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn mark_failed(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE trouble_schedules SET status = 'failed' WHERE id = $1 AND status = 'pending'",
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}

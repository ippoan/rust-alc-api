use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::{CreateTroubleTask, TroubleTask, UpdateTroubleTask};
use alc_core::tenant::TenantConn;

pub use alc_core::repository::trouble_tasks::*;

pub struct PgTroubleTasksRepository {
    pool: PgPool,
}

impl PgTroubleTasksRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TroubleTasksRepository for PgTroubleTasksRepository {
    async fn create(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
        created_by: Option<Uuid>,
        input: &CreateTroubleTask,
    ) -> Result<TroubleTask, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleTask>(
            r#"INSERT INTO trouble_tasks (tenant_id, ticket_id, task_type, title, description, assigned_to, due_date, sort_order, created_by, next_action, next_action_by, next_action_due)
            VALUES ($1, $2, $3, $4, $5, $6, $7, COALESCE($8, 0), $9, COALESCE($10, ''), $11, $12)
            RETURNING *"#,
        )
        .bind(tenant_id)
        .bind(ticket_id)
        .bind(&input.task_type)
        .bind(&input.title)
        .bind(&input.description)
        .bind(input.assigned_to)
        .bind(input.due_date)
        .bind(input.sort_order)
        .bind(created_by)
        .bind(&input.next_action)
        .bind(input.next_action_by)
        .bind(input.next_action_due)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn list_by_ticket(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
    ) -> Result<Vec<TroubleTask>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleTask>(
            "SELECT * FROM trouble_tasks WHERE ticket_id = $1 AND tenant_id = $2 ORDER BY sort_order, created_at",
        )
        .bind(ticket_id)
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<TroubleTask>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleTask>(
            "SELECT * FROM trouble_tasks WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateTroubleTask,
    ) -> Result<Option<TroubleTask>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TroubleTask>(
            r#"UPDATE trouble_tasks SET
                task_type = COALESCE($3, task_type),
                title = COALESCE($4, title),
                description = COALESCE($5, description),
                status = COALESCE($6, status),
                assigned_to = CASE WHEN $7::boolean THEN $8 ELSE assigned_to END,
                due_date = CASE WHEN $9::boolean THEN $10 ELSE due_date END,
                completed_at = CASE WHEN $11::boolean THEN $12 ELSE completed_at END,
                sort_order = COALESCE($13, sort_order),
                next_action = COALESCE($14, next_action),
                next_action_by = CASE WHEN $15::boolean THEN $16 ELSE next_action_by END,
                next_action_due = CASE WHEN $17::boolean THEN $18 ELSE next_action_due END,
                updated_at = now()
            WHERE id = $1 AND tenant_id = $2
            RETURNING *"#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(&input.task_type)
        .bind(&input.title)
        .bind(&input.description)
        .bind(&input.status)
        .bind(input.assigned_to.is_some())
        .bind(input.assigned_to.as_ref().and_then(|v| *v))
        .bind(input.due_date.is_some())
        .bind(input.due_date.as_ref().and_then(|v| *v))
        .bind(input.completed_at.is_some())
        .bind(input.completed_at.as_ref().and_then(|v| *v))
        .bind(input.sort_order)
        .bind(&input.next_action)
        .bind(input.next_action_by.is_some())
        .bind(input.next_action_by.as_ref().and_then(|v| *v))
        .bind(input.next_action_due.is_some())
        .bind(input.next_action_due.as_ref().and_then(|v| *v))
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query("DELETE FROM trouble_tasks WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(tenant_id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

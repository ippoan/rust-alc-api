use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::{
    CreateTenkoSchedule, TenkoSchedule, TenkoScheduleFilter, UpdateTenkoSchedule,
};

use super::TenantConn;

pub use alc_core::repository::tenko_schedules::*;

pub struct PgTenkoSchedulesRepository {
    pool: PgPool,
}

impl PgTenkoSchedulesRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TenkoSchedulesRepository for PgTenkoSchedulesRepository {
    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateTenkoSchedule,
    ) -> Result<TenkoSchedule, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TenkoSchedule>(
            r#"
            INSERT INTO tenko_schedules (
                tenant_id, employee_id, tenko_type,
                responsible_manager_name, scheduled_at, instruction
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(input.employee_id)
        .bind(&input.tenko_type)
        .bind(&input.responsible_manager_name)
        .bind(input.scheduled_at)
        .bind(&input.instruction)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn batch_create(
        &self,
        tenant_id: Uuid,
        inputs: &[CreateTenkoSchedule],
    ) -> Result<Vec<TenkoSchedule>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let mut results = Vec::with_capacity(inputs.len());
        for s in inputs {
            let schedule = sqlx::query_as::<_, TenkoSchedule>(
                r#"
                INSERT INTO tenko_schedules (
                    tenant_id, employee_id, tenko_type,
                    responsible_manager_name, scheduled_at, instruction
                )
                VALUES ($1, $2, $3, $4, $5, $6)
                RETURNING *
                "#,
            )
            .bind(tenant_id)
            .bind(s.employee_id)
            .bind(&s.tenko_type)
            .bind(&s.responsible_manager_name)
            .bind(s.scheduled_at)
            .bind(&s.instruction)
            .fetch_one(&mut *tc.conn)
            .await?;
            results.push(schedule);
        }
        Ok(results)
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        filter: &TenkoScheduleFilter,
        page: i64,
        per_page: i64,
    ) -> Result<ScheduleListResult, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let offset = (page - 1) * per_page;

        let mut conditions = vec!["s.tenant_id = $1".to_string()];
        let mut param_idx = 2u32;

        if filter.employee_id.is_some() {
            conditions.push(format!("s.employee_id = ${param_idx}"));
            param_idx += 1;
        }
        if filter.tenko_type.is_some() {
            conditions.push(format!("s.tenko_type = ${param_idx}"));
            param_idx += 1;
        }
        if filter.consumed.is_some() {
            conditions.push(format!("s.consumed = ${param_idx}"));
            param_idx += 1;
        }
        if filter.date_from.is_some() {
            conditions.push(format!("s.scheduled_at >= ${param_idx}"));
            param_idx += 1;
        }
        if filter.date_to.is_some() {
            conditions.push(format!("s.scheduled_at <= ${param_idx}"));
            param_idx += 1;
        }

        let where_clause = conditions.join(" AND ");

        // Count
        let count_sql = format!("SELECT COUNT(*) FROM tenko_schedules s WHERE {where_clause}");
        let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql).bind(tenant_id);
        if let Some(employee_id) = filter.employee_id {
            count_query = count_query.bind(employee_id);
        }
        if let Some(ref tenko_type) = filter.tenko_type {
            count_query = count_query.bind(tenko_type);
        }
        if let Some(consumed) = filter.consumed {
            count_query = count_query.bind(consumed);
        }
        if let Some(date_from) = filter.date_from {
            count_query = count_query.bind(date_from);
        }
        if let Some(date_to) = filter.date_to {
            count_query = count_query.bind(date_to);
        }
        let total = count_query.fetch_one(&mut *tc.conn).await?;

        // Data
        let sql = format!(
            "SELECT s.* FROM tenko_schedules s WHERE {where_clause} ORDER BY s.scheduled_at DESC LIMIT ${param_idx} OFFSET ${}",
            param_idx + 1
        );
        let mut query = sqlx::query_as::<_, TenkoSchedule>(&sql).bind(tenant_id);
        if let Some(employee_id) = filter.employee_id {
            query = query.bind(employee_id);
        }
        if let Some(ref tenko_type) = filter.tenko_type {
            query = query.bind(tenko_type);
        }
        if let Some(consumed) = filter.consumed {
            query = query.bind(consumed);
        }
        if let Some(date_from) = filter.date_from {
            query = query.bind(date_from);
        }
        if let Some(date_to) = filter.date_to {
            query = query.bind(date_to);
        }
        query = query.bind(per_page).bind(offset);

        let schedules = query.fetch_all(&mut *tc.conn).await?;

        Ok(ScheduleListResult { schedules, total })
    }

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<TenkoSchedule>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TenkoSchedule>(
            "SELECT * FROM tenko_schedules WHERE id = $1 AND tenant_id = $2",
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
        input: &UpdateTenkoSchedule,
    ) -> Result<Option<TenkoSchedule>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TenkoSchedule>(
            r#"
            UPDATE tenko_schedules SET
                responsible_manager_name = COALESCE($1, responsible_manager_name),
                scheduled_at = COALESCE($2, scheduled_at),
                instruction = COALESCE($3, instruction),
                updated_at = NOW()
            WHERE id = $4 AND tenant_id = $5 AND consumed = FALSE
            RETURNING *
            "#,
        )
        .bind(&input.responsible_manager_name)
        .bind(input.scheduled_at)
        .bind(&input.instruction)
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query(
            "DELETE FROM tenko_schedules WHERE id = $1 AND tenant_id = $2 AND consumed = FALSE",
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn get_pending(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<TenkoSchedule>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TenkoSchedule>(
            r#"
            SELECT * FROM tenko_schedules
            WHERE tenant_id = $1 AND employee_id = $2 AND consumed = FALSE
            ORDER BY scheduled_at ASC
            "#,
        )
        .bind(tenant_id)
        .bind(employee_id)
        .fetch_all(&mut *tc.conn)
        .await
    }
}

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::{
    EmployeeHealthBaseline, TenkoDashboard, TenkoRecord, TenkoSchedule, TenkoSession,
    TenkoSessionFilter,
};

use super::TenantConn;

/// Paginated list result
pub struct SessionListResult {
    pub sessions: Vec<TenkoSession>,
    pub total: i64,
}

#[allow(clippy::too_many_arguments)]
#[async_trait]
pub trait TenkoSessionRepository: Send + Sync {
    // --- Session CRUD ---

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<TenkoSession>, sqlx::Error>;

    async fn list(
        &self,
        tenant_id: Uuid,
        filter: &TenkoSessionFilter,
        page: i64,
        per_page: i64,
    ) -> Result<SessionListResult, sqlx::Error>;

    // --- Schedule queries ---

    async fn get_schedule_unconsumed(
        &self,
        tenant_id: Uuid,
        schedule_id: Uuid,
    ) -> Result<Option<TenkoSchedule>, sqlx::Error>;

    async fn consume_schedule(&self, tenant_id: Uuid, schedule_id: Uuid)
        -> Result<(), sqlx::Error>;

    async fn set_consumed_by_session(
        &self,
        tenant_id: Uuid,
        schedule_id: Uuid,
        session_id: Uuid,
    ) -> Result<(), sqlx::Error>;

    async fn get_schedule_instruction(
        &self,
        tenant_id: Uuid,
        schedule_id: Option<Uuid>,
    ) -> Result<Option<String>, sqlx::Error>;

    // --- Session creation ---

    #[allow(clippy::too_many_arguments)]
    async fn create_session(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        schedule_id: Option<Uuid>,
        tenko_type: &str,
        initial_status: &str,
        identity_face_photo_url: &Option<String>,
        location: &Option<String>,
        responsible_manager_name: &Option<String>,
    ) -> Result<TenkoSession, sqlx::Error>;

    // --- Session updates ---

    async fn update_alcohol(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        next_status: &str,
        measurement_id: Option<Uuid>,
        alcohol_result: &str,
        alcohol_value: f64,
        alcohol_face_photo_url: &Option<String>,
        cancel_reason: &Option<String>,
        completed_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<TenkoSession, sqlx::Error>;

    async fn update_medical(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        temperature: Option<f64>,
        systolic: Option<i32>,
        diastolic: Option<i32>,
        pulse: Option<i32>,
        medical_measured_at: Option<chrono::DateTime<chrono::Utc>>,
        medical_manual_input: Option<bool>,
    ) -> Result<TenkoSession, sqlx::Error>;

    async fn confirm_instruction(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<TenkoSession, sqlx::Error>;

    #[allow(clippy::too_many_arguments)]
    async fn update_report(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        next_status: &str,
        vehicle_road_status: &str,
        driver_alternation: &str,
        vehicle_road_audio_url: &Option<String>,
        driver_alternation_audio_url: &Option<String>,
        completed_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<TenkoSession, sqlx::Error>;

    async fn cancel(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        reason: &Option<String>,
    ) -> Result<TenkoSession, sqlx::Error>;

    async fn update_self_declaration(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        declaration_json: &serde_json::Value,
    ) -> Result<TenkoSession, sqlx::Error>;

    async fn update_safety_judgment(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        next_status: &str,
        judgment_json: &serde_json::Value,
        interrupted_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<TenkoSession, sqlx::Error>;

    async fn update_daily_inspection(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        next_status: &str,
        inspection_json: &serde_json::Value,
        cancel_reason: &Option<String>,
        completed_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<TenkoSession, sqlx::Error>;

    async fn update_carrying_items(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        carrying_json: &serde_json::Value,
    ) -> Result<TenkoSession, sqlx::Error>;

    async fn interrupt(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        reason: &Option<String>,
    ) -> Result<TenkoSession, sqlx::Error>;

    async fn resume(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        resume_to: &str,
        reason: &str,
        resumed_by_user_id: Option<Uuid>,
    ) -> Result<TenkoSession, sqlx::Error>;

    // --- Carrying items helpers ---

    async fn get_carrying_item_name(
        &self,
        tenant_id: Uuid,
        item_id: Uuid,
    ) -> Result<Option<String>, sqlx::Error>;

    async fn upsert_carrying_item_check(
        &self,
        tenant_id: Uuid,
        session_id: Uuid,
        item_id: Uuid,
        item_name: &str,
        checked: bool,
        checked_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<(), sqlx::Error>;

    async fn count_carrying_items(&self, tenant_id: Uuid) -> Result<i64, sqlx::Error>;

    // --- Employee / Baseline lookups ---

    async fn get_employee_name(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Option<String>, sqlx::Error>;

    async fn get_health_baseline(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Option<EmployeeHealthBaseline>, sqlx::Error>;

    // --- Tenko Record ---

    #[allow(clippy::too_many_arguments)]
    async fn create_tenko_record(
        &self,
        tenant_id: Uuid,
        session: &TenkoSession,
        employee_name: &str,
        instruction: &Option<String>,
        record_data: &serde_json::Value,
        record_hash: &str,
    ) -> Result<TenkoRecord, sqlx::Error>;

    // --- Dashboard ---

    async fn dashboard(
        &self,
        tenant_id: Uuid,
        overdue_minutes: i64,
    ) -> Result<TenkoDashboard, sqlx::Error>;
}

pub struct PgTenkoSessionRepository {
    pool: PgPool,
}

impl PgTenkoSessionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TenkoSessionRepository for PgTenkoSessionRepository {
    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<TenkoSession>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TenkoSession>(
            "SELECT * FROM tenko_sessions WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        filter: &TenkoSessionFilter,
        page: i64,
        per_page: i64,
    ) -> Result<SessionListResult, sqlx::Error> {
        let offset = (page - 1) * per_page;
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;

        let mut conditions = vec!["s.tenant_id = $1".to_string()];
        let mut param_idx = 2u32;

        if filter.employee_id.is_some() {
            conditions.push(format!("s.employee_id = ${param_idx}"));
            param_idx += 1;
        }
        if filter.status.is_some() {
            conditions.push(format!("s.status = ${param_idx}"));
            param_idx += 1;
        }
        if filter.tenko_type.is_some() {
            conditions.push(format!("s.tenko_type = ${param_idx}"));
            param_idx += 1;
        }
        if filter.date_from.is_some() {
            conditions.push(format!("s.started_at >= ${param_idx}"));
            param_idx += 1;
        }
        if filter.date_to.is_some() {
            conditions.push(format!("s.started_at <= ${param_idx}"));
            param_idx += 1;
        }

        let where_clause = conditions.join(" AND ");

        let count_sql = format!("SELECT COUNT(*) FROM tenko_sessions s WHERE {where_clause}");
        let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql).bind(tenant_id);
        if let Some(employee_id) = filter.employee_id {
            count_query = count_query.bind(employee_id);
        }
        if let Some(ref status) = filter.status {
            count_query = count_query.bind(status);
        }
        if let Some(ref tenko_type) = filter.tenko_type {
            count_query = count_query.bind(tenko_type);
        }
        if let Some(date_from) = filter.date_from {
            count_query = count_query.bind(date_from);
        }
        if let Some(date_to) = filter.date_to {
            count_query = count_query.bind(date_to);
        }
        let total = count_query.fetch_one(&mut *tc.conn).await?;

        let sql = format!(
            "SELECT s.* FROM tenko_sessions s WHERE {where_clause} ORDER BY s.created_at DESC LIMIT ${param_idx} OFFSET ${}",
            param_idx + 1
        );
        let mut query = sqlx::query_as::<_, TenkoSession>(&sql).bind(tenant_id);
        if let Some(employee_id) = filter.employee_id {
            query = query.bind(employee_id);
        }
        if let Some(ref status) = filter.status {
            query = query.bind(status);
        }
        if let Some(ref tenko_type) = filter.tenko_type {
            query = query.bind(tenko_type);
        }
        if let Some(date_from) = filter.date_from {
            query = query.bind(date_from);
        }
        if let Some(date_to) = filter.date_to {
            query = query.bind(date_to);
        }
        query = query.bind(per_page).bind(offset);

        let sessions = query.fetch_all(&mut *tc.conn).await?;

        Ok(SessionListResult { sessions, total })
    }

    async fn get_schedule_unconsumed(
        &self,
        tenant_id: Uuid,
        schedule_id: Uuid,
    ) -> Result<Option<TenkoSchedule>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TenkoSchedule>(
            "SELECT * FROM tenko_schedules WHERE id = $1 AND tenant_id = $2 AND consumed = FALSE",
        )
        .bind(schedule_id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn consume_schedule(
        &self,
        tenant_id: Uuid,
        schedule_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query("UPDATE tenko_schedules SET consumed = TRUE, updated_at = NOW() WHERE id = $1 AND tenant_id = $2")
            .bind(schedule_id)
            .bind(tenant_id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(())
    }

    async fn set_consumed_by_session(
        &self,
        tenant_id: Uuid,
        schedule_id: Uuid,
        session_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query("UPDATE tenko_schedules SET consumed_by_session_id = $1 WHERE id = $2 AND tenant_id = $3")
            .bind(session_id)
            .bind(schedule_id)
            .bind(tenant_id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(())
    }

    async fn get_schedule_instruction(
        &self,
        tenant_id: Uuid,
        schedule_id: Option<Uuid>,
    ) -> Result<Option<String>, sqlx::Error> {
        let Some(sid) = schedule_id else {
            return Ok(None);
        };
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let instruction: Option<String> =
            sqlx::query_scalar("SELECT instruction FROM tenko_schedules WHERE id = $1")
                .bind(sid)
                .fetch_optional(&mut *tc.conn)
                .await?
                .flatten();
        Ok(instruction)
    }

    async fn create_session(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        schedule_id: Option<Uuid>,
        tenko_type: &str,
        initial_status: &str,
        identity_face_photo_url: &Option<String>,
        location: &Option<String>,
        responsible_manager_name: &Option<String>,
    ) -> Result<TenkoSession, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TenkoSession>(
            r#"
            INSERT INTO tenko_sessions (
                tenant_id, employee_id, schedule_id, tenko_type, status,
                identity_verified_at, identity_face_photo_url, location,
                responsible_manager_name, started_at
            )
            VALUES ($1, $2, $3, $4, $8, NOW(), $5, $6, $7, NOW())
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(employee_id)
        .bind(schedule_id)
        .bind(tenko_type)
        .bind(identity_face_photo_url)
        .bind(location)
        .bind(responsible_manager_name)
        .bind(initial_status)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn update_alcohol(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        next_status: &str,
        measurement_id: Option<Uuid>,
        alcohol_result: &str,
        alcohol_value: f64,
        alcohol_face_photo_url: &Option<String>,
        cancel_reason: &Option<String>,
        completed_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<TenkoSession, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TenkoSession>(
            r#"
            UPDATE tenko_sessions SET
                status = $1,
                measurement_id = $2,
                alcohol_result = $3,
                alcohol_value = $4,
                alcohol_tested_at = NOW(),
                alcohol_face_photo_url = $5,
                cancel_reason = $6,
                completed_at = $7,
                updated_at = NOW()
            WHERE id = $8 AND tenant_id = $9
            RETURNING *
            "#,
        )
        .bind(next_status)
        .bind(measurement_id)
        .bind(alcohol_result)
        .bind(alcohol_value)
        .bind(alcohol_face_photo_url)
        .bind(cancel_reason)
        .bind(completed_at)
        .bind(id)
        .bind(tenant_id)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn update_medical(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        temperature: Option<f64>,
        systolic: Option<i32>,
        diastolic: Option<i32>,
        pulse: Option<i32>,
        medical_measured_at: Option<chrono::DateTime<chrono::Utc>>,
        medical_manual_input: Option<bool>,
    ) -> Result<TenkoSession, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TenkoSession>(
            r#"
            UPDATE tenko_sessions SET
                status = 'self_declaration_pending',
                temperature = COALESCE($1, temperature),
                systolic = COALESCE($2, systolic),
                diastolic = COALESCE($3, diastolic),
                pulse = COALESCE($4, pulse),
                medical_measured_at = COALESCE($5, NOW()),
                medical_manual_input = $6,
                updated_at = NOW()
            WHERE id = $7 AND tenant_id = $8
            RETURNING *
            "#,
        )
        .bind(temperature)
        .bind(systolic)
        .bind(diastolic)
        .bind(pulse)
        .bind(medical_measured_at)
        .bind(medical_manual_input)
        .bind(id)
        .bind(tenant_id)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn confirm_instruction(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<TenkoSession, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TenkoSession>(
            r#"
            UPDATE tenko_sessions SET
                status = 'completed',
                instruction_confirmed_at = NOW(),
                completed_at = NOW(),
                updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn update_report(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        next_status: &str,
        vehicle_road_status: &str,
        driver_alternation: &str,
        vehicle_road_audio_url: &Option<String>,
        driver_alternation_audio_url: &Option<String>,
        completed_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<TenkoSession, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TenkoSession>(
            r#"
            UPDATE tenko_sessions SET
                status = $1,
                report_vehicle_road_status = $2,
                report_driver_alternation = $3,
                report_vehicle_road_audio_url = $4,
                report_driver_alternation_audio_url = $5,
                report_submitted_at = NOW(),
                completed_at = $6,
                updated_at = NOW()
            WHERE id = $7 AND tenant_id = $8
            RETURNING *
            "#,
        )
        .bind(next_status)
        .bind(vehicle_road_status)
        .bind(driver_alternation)
        .bind(vehicle_road_audio_url)
        .bind(driver_alternation_audio_url)
        .bind(completed_at)
        .bind(id)
        .bind(tenant_id)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn cancel(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        reason: &Option<String>,
    ) -> Result<TenkoSession, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TenkoSession>(
            r#"
            UPDATE tenko_sessions SET
                status = 'cancelled',
                cancel_reason = $1,
                completed_at = NOW(),
                updated_at = NOW()
            WHERE id = $2 AND tenant_id = $3
            RETURNING *
            "#,
        )
        .bind(reason)
        .bind(id)
        .bind(tenant_id)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn update_self_declaration(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        declaration_json: &serde_json::Value,
    ) -> Result<TenkoSession, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TenkoSession>(
            r#"
            UPDATE tenko_sessions SET
                self_declaration = $1,
                updated_at = NOW()
            WHERE id = $2 AND tenant_id = $3
            RETURNING *
            "#,
        )
        .bind(declaration_json)
        .bind(id)
        .bind(tenant_id)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn update_safety_judgment(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        next_status: &str,
        judgment_json: &serde_json::Value,
        interrupted_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<TenkoSession, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TenkoSession>(
            r#"
            UPDATE tenko_sessions SET
                status = $1,
                safety_judgment = $2,
                interrupted_at = COALESCE($3, interrupted_at),
                updated_at = NOW()
            WHERE id = $4 AND tenant_id = $5
            RETURNING *
            "#,
        )
        .bind(next_status)
        .bind(judgment_json)
        .bind(interrupted_at)
        .bind(id)
        .bind(tenant_id)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn update_daily_inspection(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        next_status: &str,
        inspection_json: &serde_json::Value,
        cancel_reason: &Option<String>,
        completed_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<TenkoSession, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TenkoSession>(
            r#"
            UPDATE tenko_sessions SET
                status = $1,
                daily_inspection = $2,
                cancel_reason = COALESCE($3, cancel_reason),
                completed_at = COALESCE($4, completed_at),
                updated_at = NOW()
            WHERE id = $5 AND tenant_id = $6
            RETURNING *
            "#,
        )
        .bind(next_status)
        .bind(inspection_json)
        .bind(cancel_reason)
        .bind(completed_at)
        .bind(id)
        .bind(tenant_id)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn update_carrying_items(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        carrying_json: &serde_json::Value,
    ) -> Result<TenkoSession, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TenkoSession>(
            r#"UPDATE tenko_sessions SET
                status = 'identity_verified',
                carrying_items_checked = $1,
                updated_at = NOW()
            WHERE id = $2 AND tenant_id = $3
            RETURNING *"#,
        )
        .bind(carrying_json)
        .bind(id)
        .bind(tenant_id)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn interrupt(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        reason: &Option<String>,
    ) -> Result<TenkoSession, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TenkoSession>(
            r#"
            UPDATE tenko_sessions SET
                status = 'interrupted',
                interrupted_at = NOW(),
                cancel_reason = COALESCE($1, cancel_reason),
                updated_at = NOW()
            WHERE id = $2 AND tenant_id = $3
            RETURNING *
            "#,
        )
        .bind(reason)
        .bind(id)
        .bind(tenant_id)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn resume(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        resume_to: &str,
        reason: &str,
        resumed_by_user_id: Option<Uuid>,
    ) -> Result<TenkoSession, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TenkoSession>(
            r#"
            UPDATE tenko_sessions SET
                status = $1,
                resumed_at = NOW(),
                resume_reason = $2,
                resumed_by_user_id = $3,
                updated_at = NOW()
            WHERE id = $4 AND tenant_id = $5
            RETURNING *
            "#,
        )
        .bind(resume_to)
        .bind(reason)
        .bind(resumed_by_user_id)
        .bind(id)
        .bind(tenant_id)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn get_carrying_item_name(
        &self,
        tenant_id: Uuid,
        item_id: Uuid,
    ) -> Result<Option<String>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_scalar("SELECT item_name FROM alc_api.carrying_items WHERE id = $1")
            .bind(item_id)
            .fetch_optional(&mut *tc.conn)
            .await
    }

    async fn upsert_carrying_item_check(
        &self,
        tenant_id: Uuid,
        session_id: Uuid,
        item_id: Uuid,
        item_name: &str,
        checked: bool,
        checked_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query(
            r#"INSERT INTO alc_api.tenko_carrying_item_checks
               (session_id, item_id, item_name, checked, checked_at)
               VALUES ($1, $2, $3, $4, $5)
               ON CONFLICT (session_id, item_id) DO UPDATE SET
               checked = EXCLUDED.checked, checked_at = EXCLUDED.checked_at"#,
        )
        .bind(session_id)
        .bind(item_id)
        .bind(item_name)
        .bind(checked)
        .bind(checked_at)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }

    async fn count_carrying_items(&self, tenant_id: Uuid) -> Result<i64, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*)::BIGINT FROM alc_api.carrying_items WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tc.conn)
        .await?;
        Ok(count)
    }

    async fn get_employee_name(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Option<String>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_scalar("SELECT name FROM employees WHERE id = $1")
            .bind(employee_id)
            .fetch_optional(&mut *tc.conn)
            .await
    }

    async fn get_health_baseline(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Option<EmployeeHealthBaseline>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, EmployeeHealthBaseline>(
            "SELECT * FROM employee_health_baselines WHERE tenant_id = $1 AND employee_id = $2",
        )
        .bind(tenant_id)
        .bind(employee_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn create_tenko_record(
        &self,
        tenant_id: Uuid,
        session: &TenkoSession,
        employee_name: &str,
        instruction: &Option<String>,
        record_data: &serde_json::Value,
        record_hash: &str,
    ) -> Result<TenkoRecord, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let has_face_photo = session.alcohol_face_photo_url.is_some();
        sqlx::query_as::<_, TenkoRecord>(
            r#"
            INSERT INTO tenko_records (
                tenant_id, session_id, employee_id, tenko_type, status,
                record_data, employee_name, responsible_manager_name,
                location, alcohol_result, alcohol_value, alcohol_has_face_photo,
                temperature, systolic, diastolic, pulse,
                instruction, instruction_confirmed_at,
                report_vehicle_road_status, report_driver_alternation, report_no_report,
                report_vehicle_road_audio_url, report_driver_alternation_audio_url,
                started_at, completed_at, record_hash,
                self_declaration, safety_judgment, daily_inspection,
                interrupted_at, resumed_at, resume_reason
            )
            VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8,
                $9, $10, $11, $12,
                $13, $14, $15, $16,
                $17, $18,
                $19, $20, $21,
                $22, $23,
                $24, $25, $26,
                $27, $28, $29,
                $30, $31, $32
            )
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(session.id)
        .bind(session.employee_id)
        .bind(&session.tenko_type)
        .bind(&session.status)
        .bind(record_data)
        .bind(employee_name)
        .bind(&session.responsible_manager_name)
        .bind(&session.location)
        .bind(&session.alcohol_result)
        .bind(session.alcohol_value)
        .bind(has_face_photo)
        .bind(session.temperature)
        .bind(session.systolic)
        .bind(session.diastolic)
        .bind(session.pulse)
        .bind(instruction)
        .bind(session.instruction_confirmed_at)
        .bind(&session.report_vehicle_road_status)
        .bind(&session.report_driver_alternation)
        .bind(session.report_no_report)
        .bind(&session.report_vehicle_road_audio_url)
        .bind(&session.report_driver_alternation_audio_url)
        .bind(session.started_at)
        .bind(session.completed_at)
        .bind(record_hash)
        .bind(&session.self_declaration)
        .bind(&session.safety_judgment)
        .bind(&session.daily_inspection)
        .bind(session.interrupted_at)
        .bind(session.resumed_at)
        .bind(&session.resume_reason)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn dashboard(
        &self,
        tenant_id: Uuid,
        overdue_minutes: i64,
    ) -> Result<TenkoDashboard, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;

        let pending_schedules: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tenko_schedules WHERE tenant_id = $1 AND consumed = FALSE",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tc.conn)
        .await?;

        let active_sessions: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tenko_sessions WHERE tenant_id = $1 AND status NOT IN ('completed', 'cancelled', 'interrupted')",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tc.conn)
        .await?;

        let interrupted_sessions: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tenko_sessions WHERE tenant_id = $1 AND status = 'interrupted'",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tc.conn)
        .await?;

        let completed_today: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tenko_sessions WHERE tenant_id = $1 AND status = 'completed' AND completed_at >= CURRENT_DATE",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tc.conn)
        .await?;

        let cancelled_today: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tenko_sessions WHERE tenant_id = $1 AND status = 'cancelled' AND completed_at >= CURRENT_DATE",
        )
        .bind(tenant_id)
        .fetch_one(&mut *tc.conn)
        .await?;

        let overdue_schedules = sqlx::query_as::<_, TenkoSchedule>(
            r#"
            SELECT * FROM tenko_schedules
            WHERE tenant_id = $1
              AND consumed = FALSE
              AND scheduled_at + ($2 || ' minutes')::INTERVAL < NOW()
            ORDER BY scheduled_at ASC
            "#,
        )
        .bind(tenant_id)
        .bind(overdue_minutes.to_string())
        .fetch_all(&mut *tc.conn)
        .await?;

        Ok(TenkoDashboard {
            pending_schedules,
            active_sessions,
            interrupted_sessions,
            completed_today,
            cancelled_today,
            overdue_schedules,
        })
    }
}

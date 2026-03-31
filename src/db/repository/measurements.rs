use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::{
    CreateMeasurement, Measurement, MeasurementFilter, StartMeasurement, UpdateMeasurement,
};

use super::TenantConn;

pub use alc_core::repository::measurements::*;

pub struct PgMeasurementsRepository {
    pool: PgPool,
}

impl PgMeasurementsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MeasurementsRepository for PgMeasurementsRepository {
    async fn start(
        &self,
        tenant_id: Uuid,
        input: &StartMeasurement,
    ) -> Result<Measurement, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Measurement>(
            r#"
            INSERT INTO measurements (tenant_id, employee_id, status)
            VALUES ($1, $2, 'started')
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(input.employee_id)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateMeasurement,
    ) -> Result<Measurement, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Measurement>(
            r#"
            INSERT INTO measurements (
                tenant_id, employee_id, alcohol_level, result,
                face_photo_url, measured_at, device_use_count,
                temperature, systolic, diastolic, pulse, medical_measured_at,
                face_verified, medical_manual_input, video_url, status
            )
            VALUES ($1, $2, $3, $4, $5, COALESCE($6, NOW()), COALESCE($7, 0),
                    $8, $9, $10, $11, $12, $13, $14, $15, 'completed')
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(input.employee_id)
        .bind(input.alcohol_value)
        .bind(&input.result_type)
        .bind(&input.face_photo_url)
        .bind(input.measured_at)
        .bind(input.device_use_count)
        .bind(input.temperature)
        .bind(input.systolic)
        .bind(input.diastolic)
        .bind(input.pulse)
        .bind(input.medical_measured_at)
        .bind(input.face_verified)
        .bind(input.medical_manual_input)
        .bind(&input.video_url)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateMeasurement,
    ) -> Result<Option<Measurement>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Measurement>(
            r#"
            UPDATE measurements SET
                status = COALESCE($1, status),
                alcohol_level = COALESCE($2, alcohol_level),
                result = COALESCE($3, result),
                face_photo_url = COALESCE($4, face_photo_url),
                measured_at = COALESCE($5, measured_at),
                device_use_count = COALESCE($6, device_use_count),
                temperature = COALESCE($7, temperature),
                systolic = COALESCE($8, systolic),
                diastolic = COALESCE($9, diastolic),
                pulse = COALESCE($10, pulse),
                medical_measured_at = COALESCE($11, medical_measured_at),
                face_verified = COALESCE($12, face_verified),
                medical_manual_input = COALESCE($13, medical_manual_input),
                video_url = COALESCE($14, video_url),
                updated_at = NOW()
            WHERE id = $15 AND tenant_id = $16
            RETURNING *
            "#,
        )
        .bind(&input.status)
        .bind(input.alcohol_value)
        .bind(&input.result_type)
        .bind(&input.face_photo_url)
        .bind(input.measured_at)
        .bind(input.device_use_count)
        .bind(input.temperature)
        .bind(input.systolic)
        .bind(input.diastolic)
        .bind(input.pulse)
        .bind(input.medical_measured_at)
        .bind(input.face_verified)
        .bind(input.medical_manual_input)
        .bind(&input.video_url)
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Measurement>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Measurement>(
            "SELECT * FROM measurements WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        filter: &MeasurementFilter,
        page: i64,
        per_page: i64,
    ) -> Result<ListResult, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let offset = (page - 1) * per_page;

        // Build dynamic WHERE clause
        let mut conditions = vec!["m.tenant_id = $1".to_string()];
        let mut param_idx = 2u32;

        if filter.employee_id.is_some() {
            conditions.push(format!("m.employee_id = ${param_idx}"));
            param_idx += 1;
        }
        if filter.result_type.is_some() {
            conditions.push(format!("m.result = ${param_idx}"));
            param_idx += 1;
        }
        if filter.date_from.is_some() {
            conditions.push(format!("m.measured_at >= ${param_idx}"));
            param_idx += 1;
        }
        if filter.date_to.is_some() {
            conditions.push(format!("m.measured_at <= ${param_idx}"));
            param_idx += 1;
        }
        if filter.status.is_some() {
            conditions.push(format!("m.status = ${param_idx}"));
            param_idx += 1;
        }

        let where_clause = conditions.join(" AND ");

        // Count query
        let count_sql = format!("SELECT COUNT(*) FROM measurements m WHERE {where_clause}");
        let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql).bind(tenant_id);
        if let Some(employee_id) = filter.employee_id {
            count_query = count_query.bind(employee_id);
        }
        if let Some(ref result_type) = filter.result_type {
            count_query = count_query.bind(result_type);
        }
        if let Some(date_from) = filter.date_from {
            count_query = count_query.bind(date_from);
        }
        if let Some(date_to) = filter.date_to {
            count_query = count_query.bind(date_to);
        }
        if let Some(ref status) = filter.status {
            count_query = count_query.bind(status);
        }
        let total = count_query.fetch_one(&mut *tc.conn).await?;

        // Data query
        let sql = format!(
            "SELECT m.* FROM measurements m WHERE {where_clause} ORDER BY m.measured_at DESC LIMIT ${param_idx} OFFSET ${}",
            param_idx + 1
        );

        let mut query = sqlx::query_as::<_, Measurement>(&sql).bind(tenant_id);
        if let Some(employee_id) = filter.employee_id {
            query = query.bind(employee_id);
        }
        if let Some(ref result_type) = filter.result_type {
            query = query.bind(result_type);
        }
        if let Some(date_from) = filter.date_from {
            query = query.bind(date_from);
        }
        if let Some(date_to) = filter.date_to {
            query = query.bind(date_to);
        }
        if let Some(ref status) = filter.status {
            query = query.bind(status);
        }
        query = query.bind(per_page).bind(offset);

        let measurements = query.fetch_all(&mut *tc.conn).await?;

        Ok(ListResult {
            measurements,
            total,
        })
    }
}

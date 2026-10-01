use async_trait::async_trait;
use sqlx::{Acquire, PgPool};
use uuid::Uuid;

use alc_core::models::{
    CreateMeasurement, Measurement, MeasurementFilter, StartMeasurement, UpdateMeasurement,
    UpdatedMeasurement,
};

use alc_core::tenant::TenantConn;
use alc_tenko::normal_tenko::NormalTenkoInput;

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
        let mut tx = tc.conn.begin().await?;
        let m = sqlx::query_as::<_, Measurement>(
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
        .fetch_one(&mut *tx)
        .await?;

        // POST は点呼方法を受けない (従来どおり通常点呼)。点呼セッションの id も返さない
        let tenko = NormalTenkoInput {
            tenko_type: input.tenko_type.as_deref(),
            tenko_method: None,
            carins_cert_no: input.carins_cert_no.as_deref(),
            carins_vehicle_id: input.carins_vehicle_id.as_deref(),
        };
        let _ = record_as_tenko_if_marked(&mut tx, &m, input.record_as_tenko, &tenko).await;
        tx.commit().await?;
        Ok(m)
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateMeasurement,
    ) -> Result<Option<UpdatedMeasurement>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let mut tx = tc.conn.begin().await?;
        let m = sqlx::query_as::<_, Measurement>(
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
        .fetch_optional(&mut *tx)
        .await?;

        let Some(measurement) = m else {
            tx.commit().await?;
            return Ok(None);
        };
        let tenko = NormalTenkoInput {
            tenko_type: input.tenko_type.as_deref(),
            tenko_method: input.tenko_method.as_deref(),
            carins_cert_no: input.carins_cert_no.as_deref(),
            carins_vehicle_id: input.carins_vehicle_id.as_deref(),
        };
        let tenko_session_id =
            record_as_tenko_if_marked(&mut tx, &measurement, input.record_as_tenko, &tenko).await;
        tx.commit().await?;
        Ok(Some(UpdatedMeasurement {
            measurement,
            tenko_session_id,
        }))
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

/// 通常点呼の印が付いた「完了した測定」から、点呼セッションと点呼記録を作る。
///
/// **測定の保存は必ず残す** のがこの関数の契約。記録の作成は内側の区切り
/// (SAVEPOINT = sqlx の入れ子 transaction) で行い、失敗したら内側だけ戻して
/// warn を残す — 外側の測定の保存はそのまま commit される
/// (Refs ippoan/alc-app#238, ippoan/alc-app-s3#135)。
///
/// 戻り値は点呼セッションの id (Refs ippoan/alc-app#387): 今回作ったもの、または
/// 同じ測定に既にあるもの (再送)。印が無い・結果が点呼にならない・記録に失敗した
/// ときは `None`。
async fn record_as_tenko_if_marked(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    m: &Measurement,
    record_as_tenko: bool,
    tenko: &NormalTenkoInput<'_>,
) -> Option<Uuid> {
    if !record_as_tenko || m.status != "completed" {
        return None;
    }

    let mut inner = match tx.begin().await {
        Ok(inner) => inner,
        Err(e) => {
            tracing::warn!(
                "通常点呼の記録を開始できませんでした (measurement {}): {e}",
                m.id
            );
            return None;
        }
    };

    match record_or_existing_session_id(&mut inner, m, tenko).await {
        Ok(session_id) => match inner.commit().await {
            Ok(()) => session_id,
            Err(e) => {
                tracing::warn!(
                    "通常点呼の記録を確定できませんでした (measurement {}): {e}",
                    m.id
                );
                None
            }
        },
        Err(e) => {
            tracing::warn!(
                "通常点呼の記録の作成に失敗しました (measurement {}): {e}",
                m.id
            );
            if let Err(e) = inner.rollback().await {
                tracing::warn!(
                    "通常点呼の記録の巻き戻しに失敗しました (measurement {}): {e}",
                    m.id
                );
            }
            None
        }
    }
}

/// 点呼セッションを作ってその id を返す。作られなかったとき (記録済みの再送、または
/// 結果が点呼にならない測定) は、同じ測定に既にある session の id を引き直す。
async fn record_or_existing_session_id(
    conn: &mut sqlx::PgConnection,
    m: &Measurement,
    tenko: &NormalTenkoInput<'_>,
) -> Result<Option<Uuid>, sqlx::Error> {
    match alc_tenko::normal_tenko::record(conn, m, tenko).await? {
        Some(session) => Ok(Some(session.id)),
        None => alc_tenko::normal_tenko::existing_session_id(conn, m.id).await,
    }
}

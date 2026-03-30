use async_trait::async_trait;
use chrono::{NaiveDate, NaiveTime};
use sqlx::PgPool;
use uuid::Uuid;

use super::TenantConn;

/// dtako_upload_history の基本情報
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UploadHistoryRecord {
    pub tenant_id: Uuid,
    pub r2_zip_key: String,
    pub filename: String,
}

/// dtako_upload_history (tenant_id, r2_zip_key) のみ
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UploadTenantAndKey {
    pub tenant_id: Uuid,
    pub r2_zip_key: String,
}

/// recalculate 用の operations 行
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DtakoOpRow {
    pub unko_no: String,
    pub reading_date: NaiveDate,
    pub operation_date: Option<NaiveDate>,
    pub departure_at: Option<chrono::DateTime<chrono::Utc>>,
    pub return_at: Option<chrono::DateTime<chrono::Utc>>,
    pub driver_cd: Option<String>,
    pub total_distance: Option<f64>,
    pub drive_time_general: Option<i32>,
    pub drive_time_highway: Option<i32>,
    pub drive_time_bypass: Option<i32>,
}

/// single-driver recalculate 用の operations 行
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DtakoDriverOpRow {
    pub unko_no: String,
    pub reading_date: NaiveDate,
    pub operation_date: Option<NaiveDate>,
    pub departure_at: Option<chrono::DateTime<chrono::Utc>>,
    pub return_at: Option<chrono::DateTime<chrono::Utc>>,
    pub total_distance: Option<f64>,
    pub drive_time_general: Option<i32>,
    pub drive_time_highway: Option<i32>,
    pub drive_time_bypass: Option<i32>,
}

/// 日別セグメントの INSERT パラメータ
pub struct InsertSegmentParams {
    pub tenant_id: Uuid,
    pub driver_id: Uuid,
    pub work_date: NaiveDate,
    pub unko_no: String,
    pub segment_index: i32,
    pub start_at: chrono::NaiveDateTime,
    pub end_at: chrono::NaiveDateTime,
    pub work_minutes: i32,
    pub labor_minutes: i32,
    pub late_night_minutes: i32,
    pub drive_minutes: i32,
    pub cargo_minutes: i32,
}

/// daily_work_hours の INSERT パラメータ
pub struct InsertDailyWorkHoursParams {
    pub tenant_id: Uuid,
    pub driver_id: Uuid,
    pub work_date: NaiveDate,
    pub start_time: NaiveTime,
    pub total_work_minutes: i32,
    pub total_drive_minutes: i32,
    pub total_rest_minutes: i32,
    pub late_night_minutes: i32,
    pub drive_minutes: i32,
    pub cargo_minutes: i32,
    pub total_distance: f64,
    pub operation_count: i32,
    pub unko_nos: Vec<String>,
    pub overlap_drive_minutes: i32,
    pub overlap_cargo_minutes: i32,
    pub overlap_break_minutes: i32,
    pub overlap_restraint_minutes: i32,
    pub ot_late_night_minutes: i32,
}

/// operations の INSERT パラメータ
#[allow(clippy::too_many_arguments)]
pub struct InsertOperationParams {
    pub tenant_id: Uuid,
    pub unko_no: String,
    pub crew_role: i32,
    pub reading_date: NaiveDate,
    pub operation_date: Option<NaiveDate>,
    pub office_id: Option<Uuid>,
    pub vehicle_id: Option<Uuid>,
    pub driver_id: Option<Uuid>,
    pub departure_at: Option<chrono::NaiveDateTime>,
    pub return_at: Option<chrono::NaiveDateTime>,
    pub garage_out_at: Option<chrono::NaiveDateTime>,
    pub garage_in_at: Option<chrono::NaiveDateTime>,
    pub meter_start: Option<f64>,
    pub meter_end: Option<f64>,
    pub total_distance: Option<f64>,
    pub drive_time_general: Option<i32>,
    pub drive_time_highway: Option<i32>,
    pub drive_time_bypass: Option<i32>,
    pub safety_score: Option<f64>,
    pub economy_score: Option<f64>,
    pub total_score: Option<f64>,
    pub raw_data: serde_json::Value,
    pub r2_key_prefix: String,
}

#[async_trait]
pub trait DtakoUploadRepository: Send + Sync {
    // --- upload_history ---
    async fn create_upload_history(
        &self,
        tenant_id: Uuid,
        upload_id: Uuid,
        filename: &str,
    ) -> Result<(), sqlx::Error>;

    async fn update_upload_completed(
        &self,
        tenant_id: Uuid,
        upload_id: Uuid,
        operations_count: i32,
    ) -> Result<(), sqlx::Error>;

    async fn update_upload_r2_key(
        &self,
        tenant_id: Uuid,
        upload_id: Uuid,
        r2_zip_key: &str,
    ) -> Result<(), sqlx::Error>;

    async fn mark_upload_failed(&self, upload_id: Uuid, error_msg: &str)
        -> Result<(), sqlx::Error>;

    async fn get_upload_history(
        &self,
        upload_id: Uuid,
    ) -> Result<Option<UploadHistoryRecord>, sqlx::Error>;

    async fn get_upload_tenant_and_key(
        &self,
        upload_id: Uuid,
    ) -> Result<Option<UploadTenantAndKey>, sqlx::Error>;

    async fn list_uploads(&self, tenant_id: Uuid) -> Result<Vec<serde_json::Value>, sqlx::Error>;

    async fn list_pending_uploads(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error>;

    async fn list_uploads_needing_split(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<(Uuid, String)>, sqlx::Error>;

    async fn fetch_zip_keys(
        &self,
        tenant_id: Uuid,
        month_start: NaiveDate,
    ) -> Result<Vec<String>, sqlx::Error>;

    // --- masters ---
    async fn upsert_office(
        &self,
        tenant_id: Uuid,
        office_cd: &str,
        office_name: &str,
    ) -> Result<Option<Uuid>, sqlx::Error>;

    async fn upsert_vehicle(
        &self,
        tenant_id: Uuid,
        vehicle_cd: &str,
        vehicle_name: &str,
    ) -> Result<Option<Uuid>, sqlx::Error>;

    async fn upsert_driver(
        &self,
        tenant_id: Uuid,
        driver_cd: &str,
        driver_name: &str,
    ) -> Result<Option<Uuid>, sqlx::Error>;

    // --- operations ---
    async fn delete_operation(
        &self,
        tenant_id: Uuid,
        unko_no: &str,
        crew_role: i32,
    ) -> Result<(), sqlx::Error>;

    async fn insert_operation(
        &self,
        tenant_id: Uuid,
        params: &InsertOperationParams,
    ) -> Result<(), sqlx::Error>;

    async fn update_has_kudgivt(
        &self,
        tenant_id: Uuid,
        unko_nos: &[String],
    ) -> Result<(), sqlx::Error>;

    // --- event classifications ---
    async fn load_event_classifications(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<(String, String)>, sqlx::Error>;

    async fn insert_event_classification(
        &self,
        tenant_id: Uuid,
        event_cd: &str,
        event_name: &str,
        classification: &str,
    ) -> Result<(), sqlx::Error>;

    // --- employees lookup ---
    async fn get_employee_id_by_driver_cd(
        &self,
        tenant_id: Uuid,
        driver_cd: &str,
    ) -> Result<Option<Uuid>, sqlx::Error>;

    async fn get_driver_cd(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
    ) -> Result<Option<String>, sqlx::Error>;

    // --- daily work hours ---
    async fn delete_segments_by_unko(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        unko_no: &str,
    ) -> Result<(), sqlx::Error>;

    async fn delete_daily_hours_by_unko_nos(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        unko_nos: &[String],
    ) -> Result<(), sqlx::Error>;

    async fn delete_daily_hours_exact(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        work_date: NaiveDate,
        start_time: NaiveTime,
    ) -> Result<(), sqlx::Error>;

    async fn insert_daily_work_hours(
        &self,
        tenant_id: Uuid,
        params: &InsertDailyWorkHoursParams,
    ) -> Result<(), sqlx::Error>;

    async fn delete_segments_by_date(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        work_date: NaiveDate,
    ) -> Result<(), sqlx::Error>;

    async fn insert_segment(
        &self,
        tenant_id: Uuid,
        params: &InsertSegmentParams,
    ) -> Result<(), sqlx::Error>;

    // --- recalculate queries ---
    async fn fetch_operations_for_recalc(
        &self,
        tenant_id: Uuid,
        month_start: NaiveDate,
        fetch_end: NaiveDate,
    ) -> Result<Vec<DtakoOpRow>, sqlx::Error>;

    async fn load_driver_operations(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        month_start: NaiveDate,
        fetch_end: NaiveDate,
    ) -> Result<Vec<DtakoDriverOpRow>, sqlx::Error>;
}

pub struct PgDtakoUploadRepository {
    pool: PgPool,
}

impl PgDtakoUploadRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DtakoUploadRepository for PgDtakoUploadRepository {
    async fn create_upload_history(
        &self,
        tenant_id: Uuid,
        upload_id: Uuid,
        filename: &str,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query(
            r#"INSERT INTO alc_api.dtako_upload_history (id, tenant_id, uploaded_by, filename, status)
               VALUES ($1, $2, NULL, $3, 'processing')"#,
        )
        .bind(upload_id)
        .bind(tenant_id)
        .bind(filename)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }

    async fn update_upload_completed(
        &self,
        tenant_id: Uuid,
        upload_id: Uuid,
        operations_count: i32,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query(
            "UPDATE alc_api.dtako_upload_history SET status = 'completed', operations_count = $1 WHERE id = $2",
        )
        .bind(operations_count)
        .bind(upload_id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }

    async fn update_upload_r2_key(
        &self,
        tenant_id: Uuid,
        upload_id: Uuid,
        r2_zip_key: &str,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query("UPDATE alc_api.dtako_upload_history SET r2_zip_key = $1 WHERE id = $2")
            .bind(r2_zip_key)
            .bind(upload_id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(())
    }

    async fn mark_upload_failed(
        &self,
        upload_id: Uuid,
        error_msg: &str,
    ) -> Result<(), sqlx::Error> {
        // No tenant context needed — update by ID only
        let mut conn = self.pool.acquire().await?;
        sqlx::query(
            "UPDATE alc_api.dtako_upload_history SET status = 'failed', error_message = $1 WHERE id = $2",
        )
        .bind(error_msg)
        .bind(upload_id)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    async fn get_upload_history(
        &self,
        upload_id: Uuid,
    ) -> Result<Option<UploadHistoryRecord>, sqlx::Error> {
        let mut conn = self.pool.acquire().await?;
        sqlx::query_as::<_, UploadHistoryRecord>(
            "SELECT tenant_id, r2_zip_key, filename FROM alc_api.dtako_upload_history WHERE id = $1",
        )
        .bind(upload_id)
        .fetch_optional(&mut *conn)
        .await
    }

    async fn get_upload_tenant_and_key(
        &self,
        upload_id: Uuid,
    ) -> Result<Option<UploadTenantAndKey>, sqlx::Error> {
        let mut conn = self.pool.acquire().await?;
        sqlx::query_as::<_, UploadTenantAndKey>(
            "SELECT tenant_id, r2_zip_key FROM alc_api.dtako_upload_history WHERE id = $1",
        )
        .bind(upload_id)
        .fetch_optional(&mut *conn)
        .await
    }

    async fn list_uploads(&self, tenant_id: Uuid) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let rows = sqlx::query_as::<
            _,
            (
                Uuid,
                String,
                String,
                Option<String>,
                chrono::DateTime<chrono::Utc>,
                String,
            ),
        >(
            r#"SELECT id, filename, status, error_message, created_at, r2_zip_key
               FROM alc_api.dtako_upload_history
               WHERE tenant_id = $1
               ORDER BY created_at DESC
               LIMIT 50"#,
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, filename, status, error, created_at, r2_zip_key)| {
                serde_json::json!({
                    "id": id,
                    "filename": filename,
                    "status": status,
                    "error": error,
                    "created_at": created_at,
                    "r2_zip_key": r2_zip_key,
                })
            })
            .collect())
    }

    async fn list_pending_uploads(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let rows = sqlx::query_as::<
            _,
            (
                Uuid,
                Uuid,
                String,
                String,
                Option<String>,
                chrono::DateTime<chrono::Utc>,
            ),
        >(
            r#"SELECT id, tenant_id, filename, status, error_message, created_at
               FROM alc_api.dtako_upload_history
               WHERE status IN ('pending_retry', 'failed')
               ORDER BY created_at DESC
               LIMIT 50"#,
        )
        .fetch_all(&mut *tc.conn)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, tid, filename, status, error, created_at)| {
                serde_json::json!({
                    "id": id,
                    "tenant_id": tid,
                    "filename": filename,
                    "status": status,
                    "error_message": error,
                    "created_at": created_at.to_rfc3339(),
                })
            })
            .collect())
    }

    async fn list_uploads_needing_split(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as(
            r#"SELECT DISTINCT uh.id, uh.filename
               FROM alc_api.dtako_operations o
               JOIN alc_api.dtako_upload_history uh ON uh.tenant_id = o.tenant_id
               WHERE o.tenant_id = $1 AND o.has_kudgivt = FALSE
                 AND uh.status = 'completed'
                 AND uh.r2_zip_key IS NOT NULL
               ORDER BY uh.filename"#,
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn fetch_zip_keys(
        &self,
        tenant_id: Uuid,
        month_start: NaiveDate,
    ) -> Result<Vec<String>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_scalar(
            r#"SELECT DISTINCT r2_zip_key FROM alc_api.dtako_upload_history
               WHERE tenant_id = $1 AND status = 'completed'
                 AND created_at >= ($2::date - interval '60 days')
               ORDER BY r2_zip_key"#,
        )
        .bind(tenant_id)
        .bind(month_start)
        .fetch_all(&mut *tc.conn)
        .await
    }

    // --- masters ---

    async fn upsert_office(
        &self,
        tenant_id: Uuid,
        office_cd: &str,
        office_name: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        if office_cd.is_empty() {
            return Ok(None);
        }
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let rec = sqlx::query_as::<_, (Uuid,)>(
            r#"INSERT INTO alc_api.dtako_offices (tenant_id, office_cd, office_name)
               VALUES ($1, $2, $3)
               ON CONFLICT (tenant_id, office_cd) DO UPDATE SET office_name = EXCLUDED.office_name
               RETURNING id"#,
        )
        .bind(tenant_id)
        .bind(office_cd)
        .bind(office_name)
        .fetch_one(&mut *tc.conn)
        .await?;
        Ok(Some(rec.0))
    }

    async fn upsert_vehicle(
        &self,
        tenant_id: Uuid,
        vehicle_cd: &str,
        vehicle_name: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        if vehicle_cd.is_empty() {
            return Ok(None);
        }
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let rec = sqlx::query_as::<_, (Uuid,)>(
            r#"INSERT INTO alc_api.dtako_vehicles (tenant_id, vehicle_cd, vehicle_name)
               VALUES ($1, $2, $3)
               ON CONFLICT (tenant_id, vehicle_cd) DO UPDATE SET vehicle_name = EXCLUDED.vehicle_name
               RETURNING id"#,
        )
        .bind(tenant_id)
        .bind(vehicle_cd)
        .bind(vehicle_name)
        .fetch_one(&mut *tc.conn)
        .await?;
        Ok(Some(rec.0))
    }

    async fn upsert_driver(
        &self,
        tenant_id: Uuid,
        driver_cd: &str,
        driver_name: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        if driver_cd.is_empty() {
            return Ok(None);
        }
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let existing = sqlx::query_as::<_, (Uuid,)>(
            "SELECT id FROM alc_api.employees WHERE tenant_id = $1 AND driver_cd = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(driver_cd)
        .fetch_optional(&mut *tc.conn)
        .await?;

        if let Some(rec) = existing {
            Ok(Some(rec.0))
        } else {
            let rec = sqlx::query_as::<_, (Uuid,)>(
                r#"INSERT INTO alc_api.employees (tenant_id, driver_cd, name)
                   VALUES ($1, $2, $3)
                   RETURNING id"#,
            )
            .bind(tenant_id)
            .bind(driver_cd)
            .bind(driver_name)
            .fetch_one(&mut *tc.conn)
            .await?;
            Ok(Some(rec.0))
        }
    }

    // --- operations ---

    async fn delete_operation(
        &self,
        tenant_id: Uuid,
        unko_no: &str,
        crew_role: i32,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query(
            "DELETE FROM alc_api.dtako_operations WHERE tenant_id = $1 AND unko_no = $2 AND crew_role = $3",
        )
        .bind(tenant_id)
        .bind(unko_no)
        .bind(crew_role)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }

    async fn insert_operation(
        &self,
        tenant_id: Uuid,
        params: &InsertOperationParams,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query(
            r#"INSERT INTO alc_api.dtako_operations (
                tenant_id, unko_no, crew_role, reading_date, operation_date,
                office_id, vehicle_id, driver_id,
                departure_at, return_at, garage_out_at, garage_in_at,
                meter_start, meter_end, total_distance,
                drive_time_general, drive_time_highway, drive_time_bypass,
                safety_score, economy_score, total_score,
                raw_data, r2_key_prefix
            ) VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8,
                $9, $10, $11, $12,
                $13, $14, $15,
                $16, $17, $18,
                $19, $20, $21,
                $22, $23
            )"#,
        )
        .bind(params.tenant_id)
        .bind(&params.unko_no)
        .bind(params.crew_role)
        .bind(params.reading_date)
        .bind(params.operation_date)
        .bind(params.office_id)
        .bind(params.vehicle_id)
        .bind(params.driver_id)
        .bind(params.departure_at)
        .bind(params.return_at)
        .bind(params.garage_out_at)
        .bind(params.garage_in_at)
        .bind(params.meter_start)
        .bind(params.meter_end)
        .bind(params.total_distance)
        .bind(params.drive_time_general)
        .bind(params.drive_time_highway)
        .bind(params.drive_time_bypass)
        .bind(params.safety_score)
        .bind(params.economy_score)
        .bind(params.total_score)
        .bind(&params.raw_data)
        .bind(&params.r2_key_prefix)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }

    async fn update_has_kudgivt(
        &self,
        tenant_id: Uuid,
        unko_nos: &[String],
    ) -> Result<(), sqlx::Error> {
        if unko_nos.is_empty() {
            return Ok(());
        }
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        for chunk in unko_nos.chunks(100) {
            let placeholders: Vec<String> = chunk
                .iter()
                .enumerate()
                .map(|(i, _)| format!("${}", i + 2))
                .collect();
            let sql = format!(
                "UPDATE alc_api.dtako_operations SET has_kudgivt = TRUE WHERE tenant_id = $1 AND unko_no IN ({})",
                placeholders.join(", ")
            );
            let mut query = sqlx::query(&sql).bind(tenant_id);
            for unko_no in chunk {
                query = query.bind(unko_no);
            }
            query.execute(&mut *tc.conn).await?;
        }
        Ok(())
    }

    // --- event classifications ---

    async fn load_event_classifications(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<(String, String)>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as(
            "SELECT event_cd, classification FROM alc_api.dtako_event_classifications WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn insert_event_classification(
        &self,
        tenant_id: Uuid,
        event_cd: &str,
        event_name: &str,
        classification: &str,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query(
            r#"INSERT INTO alc_api.dtako_event_classifications (tenant_id, event_cd, event_name, classification)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (tenant_id, event_cd) DO NOTHING"#,
        )
        .bind(tenant_id)
        .bind(event_cd)
        .bind(event_name)
        .bind(classification)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }

    // --- employees lookup ---

    async fn get_employee_id_by_driver_cd(
        &self,
        tenant_id: Uuid,
        driver_cd: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let rec = sqlx::query_as::<_, (Uuid,)>(
            "SELECT id FROM alc_api.employees WHERE tenant_id = $1 AND driver_cd = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(driver_cd)
        .fetch_optional(&mut *tc.conn)
        .await?;
        Ok(rec.map(|r| r.0))
    }

    async fn get_driver_cd(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
    ) -> Result<Option<String>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_scalar(
            "SELECT driver_cd FROM alc_api.employees WHERE id = $1 AND tenant_id = $2",
        )
        .bind(driver_id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    // --- daily work hours ---

    async fn delete_segments_by_unko(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        unko_no: &str,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query(
            "DELETE FROM alc_api.dtako_daily_work_segments WHERE tenant_id = $1 AND driver_id = $2 AND unko_no = $3",
        )
        .bind(tenant_id)
        .bind(driver_id)
        .bind(unko_no)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }

    async fn delete_daily_hours_by_unko_nos(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        unko_nos: &[String],
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query(
            "DELETE FROM alc_api.dtako_daily_work_hours WHERE tenant_id = $1 AND driver_id = $2 AND unko_nos && $3",
        )
        .bind(tenant_id)
        .bind(driver_id)
        .bind(unko_nos)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }

    async fn delete_daily_hours_exact(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        work_date: NaiveDate,
        start_time: NaiveTime,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query(
            "DELETE FROM alc_api.dtako_daily_work_hours WHERE tenant_id = $1 AND driver_id = $2 AND work_date = $3 AND start_time = $4",
        )
        .bind(tenant_id)
        .bind(driver_id)
        .bind(work_date)
        .bind(start_time)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }

    async fn insert_daily_work_hours(
        &self,
        tenant_id: Uuid,
        params: &InsertDailyWorkHoursParams,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query(
            r#"INSERT INTO alc_api.dtako_daily_work_hours (
                tenant_id, driver_id, work_date, start_time,
                total_work_minutes, total_drive_minutes, total_rest_minutes,
                late_night_minutes, drive_minutes, cargo_minutes,
                total_distance, operation_count, unko_nos,
                overlap_drive_minutes, overlap_cargo_minutes,
                overlap_break_minutes, overlap_restraint_minutes,
                ot_late_night_minutes
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)"#,
        )
        .bind(params.tenant_id)
        .bind(params.driver_id)
        .bind(params.work_date)
        .bind(params.start_time)
        .bind(params.total_work_minutes)
        .bind(params.total_drive_minutes)
        .bind(params.total_rest_minutes)
        .bind(params.late_night_minutes)
        .bind(params.drive_minutes)
        .bind(params.cargo_minutes)
        .bind(params.total_distance)
        .bind(params.operation_count)
        .bind(&params.unko_nos)
        .bind(params.overlap_drive_minutes)
        .bind(params.overlap_cargo_minutes)
        .bind(params.overlap_break_minutes)
        .bind(params.overlap_restraint_minutes)
        .bind(params.ot_late_night_minutes)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }

    async fn delete_segments_by_date(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        work_date: NaiveDate,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query(
            "DELETE FROM alc_api.dtako_daily_work_segments WHERE tenant_id = $1 AND driver_id = $2 AND work_date = $3",
        )
        .bind(tenant_id)
        .bind(driver_id)
        .bind(work_date)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }

    async fn insert_segment(
        &self,
        tenant_id: Uuid,
        params: &InsertSegmentParams,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query(
            r#"INSERT INTO alc_api.dtako_daily_work_segments (
                tenant_id, driver_id, work_date, unko_no, segment_index,
                start_at, end_at, work_minutes, labor_minutes, late_night_minutes,
                drive_minutes, cargo_minutes
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"#,
        )
        .bind(params.tenant_id)
        .bind(params.driver_id)
        .bind(params.work_date)
        .bind(&params.unko_no)
        .bind(params.segment_index)
        .bind(params.start_at)
        .bind(params.end_at)
        .bind(params.work_minutes)
        .bind(params.labor_minutes)
        .bind(params.late_night_minutes)
        .bind(params.drive_minutes)
        .bind(params.cargo_minutes)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }

    // --- recalculate queries ---

    async fn fetch_operations_for_recalc(
        &self,
        tenant_id: Uuid,
        month_start: NaiveDate,
        fetch_end: NaiveDate,
    ) -> Result<Vec<DtakoOpRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, DtakoOpRow>(
            r#"SELECT DISTINCT o.unko_no, o.reading_date, o.operation_date,
                      o.departure_at, o.return_at,
                      d.driver_cd,
                      o.total_distance,
                      o.drive_time_general, o.drive_time_highway, o.drive_time_bypass
               FROM alc_api.dtako_operations o
               LEFT JOIN alc_api.employees d ON d.id = o.driver_id AND d.tenant_id = o.tenant_id
               WHERE o.tenant_id = $1
                 AND (o.operation_date >= $2 AND o.operation_date <= $3
                      OR o.reading_date >= $2 AND o.reading_date <= $3)
               ORDER BY o.reading_date, o.unko_no"#,
        )
        .bind(tenant_id)
        .bind(month_start)
        .bind(fetch_end)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn load_driver_operations(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        month_start: NaiveDate,
        fetch_end: NaiveDate,
    ) -> Result<Vec<DtakoDriverOpRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, DtakoDriverOpRow>(
            r#"SELECT DISTINCT o.unko_no, o.reading_date, o.operation_date,
                      o.departure_at, o.return_at,
                      o.total_distance,
                      o.drive_time_general, o.drive_time_highway, o.drive_time_bypass
               FROM alc_api.dtako_operations o
               WHERE o.tenant_id = $1 AND o.driver_id = $2
                 AND (o.operation_date >= $3 AND o.operation_date <= $4
                      OR o.reading_date >= $3 AND o.reading_date <= $4)
               ORDER BY o.reading_date, o.unko_no"#,
        )
        .bind(tenant_id)
        .bind(driver_id)
        .bind(month_start)
        .bind(fetch_end)
        .fetch_all(&mut *tc.conn)
        .await
    }
}

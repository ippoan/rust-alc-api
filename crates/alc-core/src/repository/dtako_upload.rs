use async_trait::async_trait;
use chrono::{NaiveDate, NaiveTime};
use uuid::Uuid;

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

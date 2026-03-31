use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SegmentRow {
    pub work_date: NaiveDate,
    pub unko_no: String,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub work_minutes: i32,
    pub drive_minutes: i32,
    pub cargo_minutes: i32,
}

#[derive(Debug, sqlx::FromRow)]
pub struct FiscalCumRow {
    pub total: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OpTimesRow {
    pub operation_date: NaiveDate,
    pub first_departure: DateTime<Utc>,
    pub last_seg_end: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DailyWorkHoursRow {
    pub work_date: NaiveDate,
    pub start_time: chrono::NaiveTime,
    pub total_work_minutes: i32,
    pub total_rest_minutes: Option<i32>,
    pub late_night_minutes: i32,
    pub drive_minutes: i32,
    pub cargo_minutes: i32,
    pub overlap_drive_minutes: i32,
    pub overlap_cargo_minutes: i32,
    pub overlap_break_minutes: i32,
    pub overlap_restraint_minutes: i32,
    pub ot_late_night_minutes: i32,
}

#[async_trait]
pub trait DtakoRestraintReportRepository: Send + Sync {
    /// ドライバー名を取得
    async fn get_driver_name(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
    ) -> Result<Option<String>, sqlx::Error>;

    /// 月間のセグメント一覧を取得
    async fn get_segments(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        month_start: NaiveDate,
        month_end: NaiveDate,
    ) -> Result<Vec<SegmentRow>, sqlx::Error>;

    /// 月間の日別作業時間を取得
    async fn get_daily_work_hours(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        month_start: NaiveDate,
        month_end: NaiveDate,
    ) -> Result<Vec<DailyWorkHoursRow>, sqlx::Error>;

    /// 前日の主運転時間を取得
    async fn get_prev_day_drive(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        prev_day: NaiveDate,
    ) -> Result<Option<i32>, sqlx::Error>;

    /// 年度累計拘束時間を取得
    async fn get_fiscal_cumulative(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        fiscal_year_start: NaiveDate,
        prev_month_end: NaiveDate,
    ) -> Result<i32, sqlx::Error>;

    /// 運行の始業・終業時刻を取得
    async fn get_operation_times(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        month_start: NaiveDate,
        month_end: NaiveDate,
    ) -> Result<Vec<OpTimesRow>, sqlx::Error>;

    /// driver_cd を持つドライバー一覧を取得 (CSV比較用)
    async fn list_drivers_with_cd(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<(Uuid, Option<String>, String)>, sqlx::Error>;
}

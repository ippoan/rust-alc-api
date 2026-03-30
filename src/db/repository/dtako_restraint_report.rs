use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::TenantConn;

// --- DB row types ---

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

pub struct PgDtakoRestraintReportRepository {
    pool: PgPool,
}

impl PgDtakoRestraintReportRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DtakoRestraintReportRepository for PgDtakoRestraintReportRepository {
    async fn get_driver_name(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
    ) -> Result<Option<String>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_scalar("SELECT name FROM alc_api.employees WHERE id = $1 AND tenant_id = $2")
            .bind(driver_id)
            .bind(tenant_id)
            .fetch_optional(&mut *tc.conn)
            .await
    }

    async fn get_segments(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        month_start: NaiveDate,
        month_end: NaiveDate,
    ) -> Result<Vec<SegmentRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, SegmentRow>(
            r#"SELECT work_date, unko_no, start_at, end_at, work_minutes, drive_minutes, cargo_minutes
               FROM alc_api.dtako_daily_work_segments
               WHERE tenant_id = $1 AND driver_id = $2
                 AND work_date >= $3 AND work_date <= $4
               ORDER BY work_date, start_at"#,
        )
        .bind(tenant_id)
        .bind(driver_id)
        .bind(month_start)
        .bind(month_end)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get_daily_work_hours(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        month_start: NaiveDate,
        month_end: NaiveDate,
    ) -> Result<Vec<DailyWorkHoursRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, DailyWorkHoursRow>(
            r#"SELECT work_date, start_time, total_work_minutes, total_rest_minutes, late_night_minutes,
                      drive_minutes, cargo_minutes,
                      overlap_drive_minutes, overlap_cargo_minutes,
                      overlap_break_minutes, overlap_restraint_minutes,
                      ot_late_night_minutes
               FROM alc_api.dtako_daily_work_hours
               WHERE tenant_id = $1 AND driver_id = $2
                 AND work_date >= $3 AND work_date <= $4
               ORDER BY work_date, start_time"#,
        )
        .bind(tenant_id)
        .bind(driver_id)
        .bind(month_start)
        .bind(month_end)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get_prev_day_drive(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        prev_day: NaiveDate,
    ) -> Result<Option<i32>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_scalar(
            r#"SELECT drive_minutes FROM alc_api.dtako_daily_work_segments
               WHERE tenant_id = $1 AND driver_id = $2 AND work_date = $3
               ORDER BY start_at LIMIT 1"#,
        )
        .bind(tenant_id)
        .bind(driver_id)
        .bind(prev_day)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn get_fiscal_cumulative(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        fiscal_year_start: NaiveDate,
        prev_month_end: NaiveDate,
    ) -> Result<i32, sqlx::Error> {
        let row = sqlx::query_as::<_, FiscalCumRow>(
            r#"SELECT COALESCE(SUM(total_work_minutes), 0)::BIGINT AS total
               FROM alc_api.dtako_daily_work_hours
               WHERE tenant_id = $1 AND driver_id = $2
                 AND work_date >= $3 AND work_date <= $4"#,
        )
        .bind(tenant_id)
        .bind(driver_id)
        .bind(fiscal_year_start)
        .bind(prev_month_end);

        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = row.fetch_one(&mut *tc.conn).await?;
        Ok(result.total.unwrap_or(0) as i32)
    }

    async fn get_operation_times(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        month_start: NaiveDate,
        month_end: NaiveDate,
    ) -> Result<Vec<OpTimesRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, OpTimesRow>(
            r#"SELECT o.operation_date,
                      MIN(o.departure_at) AS first_departure,
                      MAX(dws.end_at) AS last_seg_end
               FROM alc_api.dtako_operations o
               JOIN alc_api.dtako_daily_work_segments dws ON dws.driver_id = o.driver_id AND dws.unko_no = o.unko_no
               WHERE o.tenant_id = $1 AND o.driver_id = $2
                 AND o.operation_date >= $3 AND o.operation_date <= $4
                 AND o.departure_at IS NOT NULL
               GROUP BY o.operation_date"#,
        )
        .bind(tenant_id)
        .bind(driver_id)
        .bind(month_start)
        .bind(month_end)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn list_drivers_with_cd(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<(Uuid, Option<String>, String)>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as("SELECT id, driver_cd, name FROM alc_api.employees WHERE tenant_id = $1 AND driver_cd IS NOT NULL AND deleted_at IS NULL")
            .bind(tenant_id)
            .fetch_all(&mut *tc.conn)
            .await
    }
}

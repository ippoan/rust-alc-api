use async_trait::async_trait;
use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::{DtakoDailyWorkHours, DtakoDailyWorkSegment};

use super::TenantConn;

pub use alc_core::repository::dtako_daily_hours::*;

pub struct PgDtakoDailyHoursRepository {
    pool: PgPool,
}

impl PgDtakoDailyHoursRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DtakoDailyHoursRepository for PgDtakoDailyHoursRepository {
    async fn count(
        &self,
        tenant_id: Uuid,
        driver_id: Option<Uuid>,
        date_from: Option<NaiveDate>,
        date_to: Option<NaiveDate>,
    ) -> Result<i64, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let (total,): (i64,) = sqlx::query_as(
            r#"SELECT COUNT(*)::BIGINT FROM alc_api.dtako_daily_work_hours
               WHERE ($1::UUID IS NULL OR driver_id = $1)
                 AND ($2::DATE IS NULL OR work_date >= $2)
                 AND ($3::DATE IS NULL OR work_date <= $3)"#,
        )
        .bind(driver_id)
        .bind(date_from)
        .bind(date_to)
        .fetch_one(&mut *tc.conn)
        .await?;
        Ok(total)
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        driver_id: Option<Uuid>,
        date_from: Option<NaiveDate>,
        date_to: Option<NaiveDate>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<DtakoDailyWorkHours>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, DtakoDailyWorkHours>(
            r#"SELECT * FROM alc_api.dtako_daily_work_hours
               WHERE ($1::UUID IS NULL OR driver_id = $1)
                 AND ($2::DATE IS NULL OR work_date >= $2)
                 AND ($3::DATE IS NULL OR work_date <= $3)
               ORDER BY work_date ASC
               LIMIT $4 OFFSET $5"#,
        )
        .bind(driver_id)
        .bind(date_from)
        .bind(date_to)
        .bind(limit)
        .bind(offset)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get_segments(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        date: NaiveDate,
    ) -> Result<Vec<DtakoDailyWorkSegment>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, DtakoDailyWorkSegment>(
            r#"SELECT * FROM alc_api.dtako_daily_work_segments
               WHERE driver_id = $1 AND work_date = $2
               ORDER BY start_at"#,
        )
        .bind(driver_id)
        .bind(date)
        .fetch_all(&mut *tc.conn)
        .await
    }
}

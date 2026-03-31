use async_trait::async_trait;
use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

use super::TenantConn;

pub use alc_core::repository::dtako_work_times::*;

pub struct PgDtakoWorkTimesRepository {
    pool: PgPool,
}

impl PgDtakoWorkTimesRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DtakoWorkTimesRepository for PgDtakoWorkTimesRepository {
    async fn count(
        &self,
        tenant_id: Uuid,
        driver_id: Option<Uuid>,
        date_from: Option<NaiveDate>,
        date_to: Option<NaiveDate>,
    ) -> Result<i64, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let (total,): (i64,) = sqlx::query_as(
            r#"SELECT COUNT(*)::BIGINT FROM alc_api.dtako_daily_work_segments
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
    ) -> Result<Vec<WorkTimeItem>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, WorkTimeItem>(
            r#"SELECT s.id, s.driver_id, s.work_date, s.unko_no, s.segment_index,
                      s.start_at, s.end_at, s.work_minutes, s.labor_minutes
               FROM alc_api.dtako_daily_work_segments s
               JOIN alc_api.employees d ON d.id = s.driver_id
               WHERE ($1::UUID IS NULL OR s.driver_id = $1)
                 AND ($2::DATE IS NULL OR s.work_date >= $2)
                 AND ($3::DATE IS NULL OR s.work_date <= $3)
               ORDER BY s.work_date ASC, d.driver_cd, s.start_at
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
}

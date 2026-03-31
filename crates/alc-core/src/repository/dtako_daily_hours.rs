use async_trait::async_trait;
use chrono::NaiveDate;
use uuid::Uuid;

use crate::models::{DtakoDailyWorkHours, DtakoDailyWorkSegment};

#[async_trait]
pub trait DtakoDailyHoursRepository: Send + Sync {
    async fn count(
        &self,
        tenant_id: Uuid,
        driver_id: Option<Uuid>,
        date_from: Option<NaiveDate>,
        date_to: Option<NaiveDate>,
    ) -> Result<i64, sqlx::Error>;

    async fn list(
        &self,
        tenant_id: Uuid,
        driver_id: Option<Uuid>,
        date_from: Option<NaiveDate>,
        date_to: Option<NaiveDate>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<DtakoDailyWorkHours>, sqlx::Error>;

    async fn get_segments(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        date: NaiveDate,
    ) -> Result<Vec<DtakoDailyWorkSegment>, sqlx::Error>;
}

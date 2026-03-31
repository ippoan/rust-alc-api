use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct WorkTimesFilter {
    pub driver_id: Option<Uuid>,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct WorkTimeItem {
    pub id: Uuid,
    pub driver_id: Uuid,
    pub work_date: NaiveDate,
    pub unko_no: String,
    pub segment_index: i32,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub work_minutes: i32,
    pub labor_minutes: i32,
}

#[derive(Debug, Serialize)]
pub struct WorkTimesResponse {
    pub items: Vec<WorkTimeItem>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

#[async_trait]
pub trait DtakoWorkTimesRepository: Send + Sync {
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
    ) -> Result<Vec<WorkTimeItem>, sqlx::Error>;
}

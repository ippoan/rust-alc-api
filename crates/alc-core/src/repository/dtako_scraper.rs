use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ScrapeHistoryItem {
    pub id: Uuid,
    pub target_date: NaiveDate,
    pub comp_id: String,
    pub status: String,
    pub message: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[async_trait]
pub trait DtakoScraperRepository: Send + Sync {
    async fn insert_scrape_history(
        &self,
        tenant_id: Uuid,
        target_date: NaiveDate,
        comp_id: &str,
        status: &str,
        message: Option<&str>,
    ) -> Result<(), sqlx::Error>;

    async fn list_scrape_history(
        &self,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ScrapeHistoryItem>, sqlx::Error>;
}

use async_trait::async_trait;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct NotifyDocument {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub source_type: String,
    pub source_sender: Option<String>,
    pub source_subject: Option<String>,
    pub r2_key: String,
    pub file_name: Option<String>,
    pub file_size_bytes: Option<i64>,
    pub extracted_title: Option<String>,
    pub extracted_date: Option<chrono::NaiveDate>,
    pub extracted_summary: Option<String>,
    pub extracted_phone_numbers: Option<Vec<String>>,
    pub extracted_data: Option<serde_json::Value>,
    pub extraction_status: String,
    pub extraction_error: Option<String>,
    pub distribution_status: String,
    pub distributed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug)]
pub struct CreateNotifyDocument {
    pub source_type: String,
    pub source_sender: Option<String>,
    pub source_subject: Option<String>,
    pub r2_key: String,
    pub file_name: Option<String>,
    pub file_size_bytes: Option<i64>,
}

#[derive(Debug)]
pub struct ExtractionResult {
    pub title: Option<String>,
    pub date: Option<chrono::NaiveDate>,
    pub summary: Option<String>,
    pub phone_numbers: Vec<String>,
    pub data: serde_json::Value,
}

#[async_trait]
pub trait NotifyDocumentRepository: Send + Sync {
    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateNotifyDocument,
    ) -> Result<NotifyDocument, sqlx::Error>;

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<NotifyDocument>, sqlx::Error>;

    async fn list(
        &self,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<NotifyDocument>, sqlx::Error>;

    async fn search(
        &self,
        tenant_id: Uuid,
        query: &str,
    ) -> Result<Vec<NotifyDocument>, sqlx::Error>;

    async fn update_extraction(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        result: &ExtractionResult,
    ) -> Result<(), sqlx::Error>;

    async fn update_extraction_error(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        error: &str,
    ) -> Result<(), sqlx::Error>;

    async fn update_distribution_status(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        status: &str,
    ) -> Result<(), sqlx::Error>;
}

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct FileRow {
    pub uuid: String,
    pub filename: String,
    pub file_type: String,
    pub created: String,
    pub deleted: Option<String>,
    pub blob: Option<String>,
    pub s3_key: Option<String>,
    pub storage_class: Option<String>,
    pub last_accessed_at: Option<String>,
    pub access_count_weekly: Option<i32>,
    pub access_count_total: Option<i32>,
    pub promoted_to_standard_at: Option<String>,
}

#[async_trait]
pub trait CarinsFilesRepository: Send + Sync {
    async fn list_files(
        &self,
        tenant_id: Uuid,
        type_filter: Option<&str>,
    ) -> Result<Vec<FileRow>, sqlx::Error>;

    async fn list_recent(&self, tenant_id: Uuid) -> Result<Vec<FileRow>, sqlx::Error>;

    async fn list_not_attached(&self, tenant_id: Uuid) -> Result<Vec<FileRow>, sqlx::Error>;

    async fn get_file(&self, tenant_id: Uuid, uuid: &str) -> Result<Option<FileRow>, sqlx::Error>;

    /// Get file metadata for download (includes blob column).
    async fn get_file_for_download(
        &self,
        tenant_id: Uuid,
        uuid: &str,
    ) -> Result<Option<FileRow>, sqlx::Error>;

    async fn create_file(
        &self,
        tenant_id: Uuid,
        file_uuid: Uuid,
        filename: &str,
        file_type: &str,
        gcs_key: &str,
        now: DateTime<Utc>,
    ) -> Result<FileRow, sqlx::Error>;

    /// Soft-delete. Returns true if a row was affected.
    async fn delete_file(&self, tenant_id: Uuid, uuid: &str) -> Result<bool, sqlx::Error>;

    /// Restore soft-deleted file. Returns true if a row was affected.
    async fn restore_file(&self, tenant_id: Uuid, uuid: &str) -> Result<bool, sqlx::Error>;
}

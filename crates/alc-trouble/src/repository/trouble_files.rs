use async_trait::async_trait;
use uuid::Uuid;

use crate::models::TroubleFile;

#[async_trait]
#[allow(clippy::too_many_arguments)]
pub trait TroubleFilesRepository: Send + Sync {
    async fn create(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
        filename: &str,
        content_type: &str,
        size_bytes: i64,
        storage_key: &str,
    ) -> Result<TroubleFile, sqlx::Error>;

    async fn create_for_task(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
        task_id: Uuid,
        filename: &str,
        content_type: &str,
        size_bytes: i64,
        storage_key: &str,
    ) -> Result<TroubleFile, sqlx::Error>;

    async fn list_by_ticket(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
    ) -> Result<Vec<TroubleFile>, sqlx::Error>;

    async fn list_by_task(
        &self,
        tenant_id: Uuid,
        task_id: Uuid,
    ) -> Result<Vec<TroubleFile>, sqlx::Error>;

    async fn list_trash(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
    ) -> Result<Vec<TroubleFile>, sqlx::Error>;

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<TroubleFile>, sqlx::Error>;

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;

    async fn restore(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;
}

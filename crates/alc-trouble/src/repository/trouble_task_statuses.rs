use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{CreateTroubleTaskStatus, TroubleTaskStatus, UpdateTroubleTaskStatus};

#[async_trait]
pub trait TroubleTaskStatusesRepository: Send + Sync {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<TroubleTaskStatus>, sqlx::Error>;
    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateTroubleTaskStatus,
    ) -> Result<TroubleTaskStatus, sqlx::Error>;
    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateTroubleTaskStatus,
    ) -> Result<Option<TroubleTaskStatus>, sqlx::Error>;
    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;
}

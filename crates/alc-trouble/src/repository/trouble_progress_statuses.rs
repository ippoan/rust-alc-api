use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{CreateTroubleProgressStatus, TroubleProgressStatus};

#[async_trait]
pub trait TroubleProgressStatusesRepository: Send + Sync {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<TroubleProgressStatus>, sqlx::Error>;
    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateTroubleProgressStatus,
    ) -> Result<TroubleProgressStatus, sqlx::Error>;
    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;
    async fn update_sort_order(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        sort_order: i32,
    ) -> Result<Option<TroubleProgressStatus>, sqlx::Error>;
}

use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{CreateTroubleOffice, TroubleOffice};

#[async_trait]
pub trait TroubleOfficesRepository: Send + Sync {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<TroubleOffice>, sqlx::Error>;
    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateTroubleOffice,
    ) -> Result<TroubleOffice, sqlx::Error>;
    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;
    async fn update_sort_order(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        sort_order: i32,
    ) -> Result<Option<TroubleOffice>, sqlx::Error>;
}

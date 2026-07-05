use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{CreateTroubleCategory, TroubleCategory};

#[async_trait]
pub trait TroubleTaskTypesRepository: Send + Sync {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<TroubleCategory>, sqlx::Error>;
    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateTroubleCategory,
    ) -> Result<TroubleCategory, sqlx::Error>;
    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;
    async fn update_sort_order(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        sort_order: i32,
    ) -> Result<Option<TroubleCategory>, sqlx::Error>;
}

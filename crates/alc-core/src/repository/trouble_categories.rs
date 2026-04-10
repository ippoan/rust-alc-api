use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{CreateTroubleCategory, TroubleCategory};

#[async_trait]
pub trait TroubleCategoriesRepository: Send + Sync {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<TroubleCategory>, sqlx::Error>;
    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateTroubleCategory,
    ) -> Result<TroubleCategory, sqlx::Error>;
    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;
}

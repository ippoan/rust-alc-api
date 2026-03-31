use async_trait::async_trait;
use uuid::Uuid;

use crate::models::DtakoEventClassification;

#[async_trait]
pub trait DtakoEventClassificationsRepository: Send + Sync {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<DtakoEventClassification>, sqlx::Error>;

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        classification: &str,
    ) -> Result<Option<DtakoEventClassification>, sqlx::Error>;
}

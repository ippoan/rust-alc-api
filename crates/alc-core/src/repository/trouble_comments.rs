use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{CreateTroubleComment, TroubleComment};

#[async_trait]
pub trait TroubleCommentsRepository: Send + Sync {
    async fn create(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
        author_id: Option<Uuid>,
        input: &CreateTroubleComment,
    ) -> Result<TroubleComment, sqlx::Error>;

    async fn list_by_ticket(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
    ) -> Result<Vec<TroubleComment>, sqlx::Error>;

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;
}

use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{CreateTroubleTaskActivity, TroubleTaskActivity, UpdateTroubleTaskActivity};

#[async_trait]
pub trait TroubleTaskActivitiesRepository: Send + Sync {
    async fn create(
        &self,
        tenant_id: Uuid,
        task_id: Uuid,
        created_by: Option<Uuid>,
        input: &CreateTroubleTaskActivity,
    ) -> Result<TroubleTaskActivity, sqlx::Error>;

    async fn list_by_task(
        &self,
        tenant_id: Uuid,
        task_id: Uuid,
    ) -> Result<Vec<TroubleTaskActivity>, sqlx::Error>;

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateTroubleTaskActivity,
    ) -> Result<Option<TroubleTaskActivity>, sqlx::Error>;

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;
}

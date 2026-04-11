use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{CreateTroubleSchedule, TroubleSchedule};

#[async_trait]
pub trait TroubleSchedulesRepository: Send + Sync {
    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateTroubleSchedule,
        created_by: Option<Uuid>,
    ) -> Result<TroubleSchedule, sqlx::Error>;

    async fn list_by_ticket(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
    ) -> Result<Vec<TroubleSchedule>, sqlx::Error>;

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<TroubleSchedule>, sqlx::Error>;

    async fn update_status(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        status: &str,
    ) -> Result<bool, sqlx::Error>;

    async fn set_cloud_task_name(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        task_name: &str,
    ) -> Result<bool, sqlx::Error>;

    /// RLS バイパス — Cloud Tasks fire 用 (SECURITY DEFINER 関数経由)
    async fn get_for_fire(&self, id: Uuid) -> Result<Option<TroubleSchedule>, sqlx::Error>;

    /// fire 後の status + sent_at 更新 (RLS バイパス)
    async fn mark_sent(&self, id: Uuid) -> Result<bool, sqlx::Error>;

    async fn mark_failed(&self, id: Uuid) -> Result<bool, sqlx::Error>;
}

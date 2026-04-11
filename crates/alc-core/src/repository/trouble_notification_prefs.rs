use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{TroubleNotificationPref, UpsertNotificationPref};

#[async_trait]
pub trait TroubleNotificationPrefsRepository: Send + Sync {
    async fn upsert(
        &self,
        tenant_id: Uuid,
        input: &UpsertNotificationPref,
    ) -> Result<TroubleNotificationPref, sqlx::Error>;

    async fn list(&self, tenant_id: Uuid) -> Result<Vec<TroubleNotificationPref>, sqlx::Error>;

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;

    async fn find_enabled(
        &self,
        tenant_id: Uuid,
        event_type: &str,
        channel: &str,
    ) -> Result<Option<TroubleNotificationPref>, sqlx::Error>;
}

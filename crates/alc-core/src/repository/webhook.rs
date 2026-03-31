use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{TenkoSchedule, WebhookConfig};

#[async_trait]
#[allow(clippy::too_many_arguments)]
pub trait WebhookRepository: Send + Sync {
    async fn find_config(
        &self,
        tenant_id: Uuid,
        event_type: &str,
    ) -> Result<Option<WebhookConfig>, sqlx::Error>;

    async fn record_delivery(
        &self,
        tenant_id: Uuid,
        config_id: Uuid,
        event_type: &str,
        payload: &serde_json::Value,
        status_code: Option<i32>,
        response_body: Option<&str>,
        attempt: i32,
        success: bool,
    ) -> Result<(), sqlx::Error>;

    async fn find_overdue_configs(&self) -> Result<Vec<WebhookConfig>, sqlx::Error>;

    async fn find_overdue_schedules(
        &self,
        tenant_id: Uuid,
        overdue_minutes: i64,
    ) -> Result<Vec<TenkoSchedule>, sqlx::Error>;

    async fn get_employee_name(&self, employee_id: Uuid) -> Result<Option<String>, sqlx::Error>;

    async fn mark_overdue_notified(&self, schedule_id: Uuid) -> Result<(), sqlx::Error>;
}

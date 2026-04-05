use async_trait::async_trait;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct NotifyRecipient {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub provider: String,
    pub lineworks_user_id: Option<String>,
    pub line_user_id: Option<String>,
    pub phone_number: Option<String>,
    pub email: Option<String>,
    pub enabled: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, serde::Deserialize)]
pub struct CreateNotifyRecipient {
    pub name: String,
    pub provider: String,
    pub lineworks_user_id: Option<String>,
    pub line_user_id: Option<String>,
    pub phone_number: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct UpdateNotifyRecipient {
    pub name: Option<String>,
    pub provider: Option<String>,
    pub lineworks_user_id: Option<String>,
    pub line_user_id: Option<String>,
    pub phone_number: Option<String>,
    pub email: Option<String>,
    pub enabled: Option<bool>,
}

#[async_trait]
pub trait NotifyRecipientRepository: Send + Sync {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<NotifyRecipient>, sqlx::Error>;

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<NotifyRecipient>, sqlx::Error>;

    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateNotifyRecipient,
    ) -> Result<NotifyRecipient, sqlx::Error>;

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateNotifyRecipient,
    ) -> Result<NotifyRecipient, sqlx::Error>;

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<(), sqlx::Error>;

    async fn list_enabled(&self, tenant_id: Uuid) -> Result<Vec<NotifyRecipient>, sqlx::Error>;

    /// LINE Bot webhook follow イベントで user_id を自動登録 (upsert)
    async fn upsert_by_line_user_id(
        &self,
        tenant_id: Uuid,
        line_user_id: &str,
        name: &str,
    ) -> Result<NotifyRecipient, sqlx::Error>;
}

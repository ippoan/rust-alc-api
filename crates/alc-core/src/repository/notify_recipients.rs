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

#[async_trait]
pub trait NotifyRecipientRepository: Send + Sync {
    /// LINE Bot webhook follow イベントで user_id を自動登録 (upsert)
    async fn upsert_by_line_user_id(
        &self,
        tenant_id: Uuid,
        line_user_id: &str,
        name: &str,
    ) -> Result<NotifyRecipient, sqlx::Error>;
}

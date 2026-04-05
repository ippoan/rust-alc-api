use async_trait::async_trait;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct NotifyDelivery {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub document_id: Uuid,
    pub recipient_id: Uuid,
    pub provider: String,
    pub status: String,
    pub error_message: Option<String>,
    pub attempt: i32,
    pub sent_at: Option<chrono::DateTime<chrono::Utc>>,
    pub read_at: Option<chrono::DateTime<chrono::Utc>>,
    pub read_token: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 配信記録 + 受信者名 (一覧表示用)
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct NotifyDeliveryWithRecipient {
    pub id: Uuid,
    pub document_id: Uuid,
    pub recipient_id: Uuid,
    pub provider: String,
    pub status: String,
    pub error_message: Option<String>,
    pub attempt: i32,
    pub sent_at: Option<chrono::DateTime<chrono::Utc>>,
    pub read_at: Option<chrono::DateTime<chrono::Utc>>,
    pub read_token: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub recipient_name: String,
}

/// mark_delivery_read の戻り値
#[derive(Debug, sqlx::FromRow)]
pub struct ReadResult {
    pub document_id: Uuid,
    pub tenant_id: Uuid,
}

#[async_trait]
pub trait NotifyDeliveryRepository: Send + Sync {
    /// ドキュメントに対して全受信者分の配信レコードを一括作成
    async fn create_batch(
        &self,
        tenant_id: Uuid,
        document_id: Uuid,
        recipients: &[(Uuid, String)], // (recipient_id, provider)
    ) -> Result<Vec<NotifyDelivery>, sqlx::Error>;

    async fn update_status(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        status: &str,
        error: Option<&str>,
    ) -> Result<(), sqlx::Error>;

    async fn mark_sent(&self, tenant_id: Uuid, id: Uuid) -> Result<(), sqlx::Error>;

    /// 既読記録 (SECURITY DEFINER 関数経由、テナント不要)
    async fn mark_read(&self, read_token: Uuid) -> Result<Option<ReadResult>, sqlx::Error>;

    async fn list_by_document(
        &self,
        tenant_id: Uuid,
        document_id: Uuid,
    ) -> Result<Vec<NotifyDeliveryWithRecipient>, sqlx::Error>;

    async fn list_pending(
        &self,
        tenant_id: Uuid,
        document_id: Uuid,
    ) -> Result<Vec<NotifyDelivery>, sqlx::Error>;
}

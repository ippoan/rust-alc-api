use async_trait::async_trait;
use uuid::Uuid;

/// Webhook サービス trait — テスト時に mock 差し替え可能
#[async_trait]
pub trait WebhookService: Send + Sync {
    async fn fire_event(&self, tenant_id: Uuid, event_type: &str, payload: serde_json::Value);
}

/// HTTP 配信 trait — テスト時に mock 差し替え可能
#[async_trait]
pub trait WebhookHttpClient: Send + Sync {
    /// Webhook を配信し、(status_code, response_body, success) を返す
    async fn deliver(
        &self,
        url: &str,
        event_type: &str,
        payload: &serde_json::Value,
        secret: Option<&str>,
    ) -> Result<(Option<i32>, Option<String>, bool), anyhow::Error>;
}

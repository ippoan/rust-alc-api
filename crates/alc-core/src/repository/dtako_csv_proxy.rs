use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait DtakoCsvProxyRepository: Send + Sync {
    async fn get_r2_key_prefix(
        &self,
        tenant_id: Uuid,
        unko_no: &str,
    ) -> Result<Option<String>, sqlx::Error>;
}

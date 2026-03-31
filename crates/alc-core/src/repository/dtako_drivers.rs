use async_trait::async_trait;
use serde::Serialize;
use uuid::Uuid;

/// daiun-salary 互換の Driver レスポンス
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Driver {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub driver_cd: Option<String>,
    #[serde(rename = "driver_name")]
    pub driver_name: String,
}

#[async_trait]
pub trait DtakoDriversRepository: Send + Sync {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<Driver>, sqlx::Error>;
}

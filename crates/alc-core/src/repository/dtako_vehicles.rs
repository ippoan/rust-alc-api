use async_trait::async_trait;
use uuid::Uuid;

use crate::models::DtakoVehicle;

#[async_trait]
pub trait DtakoVehiclesRepository: Send + Sync {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<DtakoVehicle>, sqlx::Error>;
}

use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{
    CreateEquipmentFailure, EquipmentFailure, EquipmentFailureFilter, EquipmentFailuresResponse,
    UpdateEquipmentFailure,
};

#[async_trait]
pub trait EquipmentFailuresRepository: Send + Sync {
    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateEquipmentFailure,
    ) -> Result<EquipmentFailure, sqlx::Error>;

    async fn list(
        &self,
        tenant_id: Uuid,
        filter: &EquipmentFailureFilter,
    ) -> Result<EquipmentFailuresResponse, sqlx::Error>;

    async fn get(&self, tenant_id: Uuid, id: Uuid)
        -> Result<Option<EquipmentFailure>, sqlx::Error>;

    async fn resolve(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateEquipmentFailure,
    ) -> Result<Option<EquipmentFailure>, sqlx::Error>;

    async fn list_for_csv(
        &self,
        tenant_id: Uuid,
        filter: &EquipmentFailureFilter,
    ) -> Result<Vec<EquipmentFailure>, sqlx::Error>;
}

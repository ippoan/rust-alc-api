use async_trait::async_trait;
use uuid::Uuid;

use crate::models::NfcTag;

#[async_trait]
pub trait NfcTagRepository: Send + Sync {
    async fn search_by_uuid(
        &self,
        tenant_id: Uuid,
        nfc_uuid: &str,
    ) -> Result<Option<NfcTag>, sqlx::Error>;

    async fn get_car_inspection_json(
        &self,
        tenant_id: Uuid,
        car_inspection_id: i32,
    ) -> Result<Option<serde_json::Value>, sqlx::Error>;

    async fn list(
        &self,
        tenant_id: Uuid,
        car_inspection_id: Option<i32>,
    ) -> Result<Vec<NfcTag>, sqlx::Error>;

    async fn register(
        &self,
        tenant_id: Uuid,
        nfc_uuid: &str,
        car_inspection_id: i32,
    ) -> Result<NfcTag, sqlx::Error>;

    async fn delete(&self, tenant_id: Uuid, nfc_uuid: &str) -> Result<bool, sqlx::Error>;
}

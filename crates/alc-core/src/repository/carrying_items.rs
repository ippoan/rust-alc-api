use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{CarryingItem, CarryingItemVehicleCondition};

#[async_trait]
pub trait CarryingItemsRepository: Send + Sync {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<CarryingItem>, sqlx::Error>;

    async fn list_conditions(
        &self,
        tenant_id: Uuid,
        item_ids: &[Uuid],
    ) -> Result<Vec<CarryingItemVehicleCondition>, sqlx::Error>;

    async fn create(
        &self,
        tenant_id: Uuid,
        item_name: &str,
        is_required: bool,
        sort_order: i32,
    ) -> Result<CarryingItem, sqlx::Error>;

    async fn insert_condition(
        &self,
        tenant_id: Uuid,
        item_id: Uuid,
        category: &str,
        value: &str,
    ) -> Result<Option<CarryingItemVehicleCondition>, sqlx::Error>;

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        item_name: Option<&str>,
        is_required: Option<bool>,
        sort_order: Option<i32>,
    ) -> Result<Option<CarryingItem>, sqlx::Error>;

    async fn delete_conditions(&self, tenant_id: Uuid, item_id: Uuid) -> Result<(), sqlx::Error>;

    async fn get_conditions(
        &self,
        tenant_id: Uuid,
        item_id: Uuid,
    ) -> Result<Vec<CarryingItemVehicleCondition>, sqlx::Error>;

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;
}

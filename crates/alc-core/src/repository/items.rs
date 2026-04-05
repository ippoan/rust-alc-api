use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{CreateItem, Item, ItemFile, UpdateItem};

#[async_trait]
pub trait ItemsRepository: Send + Sync {
    async fn list(
        &self,
        tenant_id: Uuid,
        parent_id: Option<Uuid>,
        owner_type: &str,
    ) -> Result<Vec<Item>, sqlx::Error>;

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Item>, sqlx::Error>;

    async fn create(&self, tenant_id: Uuid, item: &CreateItem) -> Result<Item, sqlx::Error>;

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        item: &UpdateItem,
    ) -> Result<Option<Item>, sqlx::Error>;

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;

    async fn move_item(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        new_parent_id: Option<Uuid>,
    ) -> Result<Option<Item>, sqlx::Error>;

    async fn change_ownership(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        new_owner_type: &str,
    ) -> Result<Option<Item>, sqlx::Error>;

    async fn convert_type(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        new_item_type: &str,
    ) -> Result<Option<(Item, i64)>, sqlx::Error>;

    async fn search_by_barcode(
        &self,
        tenant_id: Uuid,
        barcode: &str,
    ) -> Result<Vec<Item>, sqlx::Error>;
}

#[async_trait]
pub trait ItemFilesRepository: Send + Sync {
    async fn create(
        &self,
        tenant_id: Uuid,
        filename: &str,
        content_type: &str,
        size_bytes: i64,
    ) -> Result<ItemFile, sqlx::Error>;

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<ItemFile>, sqlx::Error>;
}

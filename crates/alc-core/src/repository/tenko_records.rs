use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{TenkoRecord, TenkoRecordFilter};

#[async_trait]
pub trait TenkoRecordsRepository: Send + Sync {
    async fn count(&self, tenant_id: Uuid, filter: &TenkoRecordFilter) -> Result<i64, sqlx::Error>;

    async fn list(
        &self,
        tenant_id: Uuid,
        filter: &TenkoRecordFilter,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TenkoRecord>, sqlx::Error>;

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<TenkoRecord>, sqlx::Error>;

    async fn list_all(
        &self,
        tenant_id: Uuid,
        filter: &TenkoRecordFilter,
    ) -> Result<Vec<TenkoRecord>, sqlx::Error>;
}

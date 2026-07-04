use async_trait::async_trait;
use uuid::Uuid;

use crate::models::TroubleFieldLayout;

#[async_trait]
pub trait TroubleFieldLayoutsRepository: Send + Sync {
    /// レコードが存在しない場合は空の settings を持つ TroubleFieldLayout を返す
    /// (フロントエンドはこれをデフォルトメタデータで補完する)。
    async fn get(&self, tenant_id: Uuid) -> Result<TroubleFieldLayout, sqlx::Error>;

    async fn upsert(
        &self,
        tenant_id: Uuid,
        layout: &TroubleFieldLayout,
    ) -> Result<TroubleFieldLayout, sqlx::Error>;
}

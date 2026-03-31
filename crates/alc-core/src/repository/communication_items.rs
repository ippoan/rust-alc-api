use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{CommunicationItem, CreateCommunicationItem, UpdateCommunicationItem};

/// 伝達事項の一覧取得結果 (WITH name join)
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct CommunicationItemWithName {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub title: String,
    pub content: String,
    pub priority: String,
    pub target_employee_id: Option<Uuid>,
    pub target_employee_name: Option<String>,
    pub is_active: bool,
    pub effective_from: Option<chrono::DateTime<chrono::Utc>>,
    pub effective_until: Option<chrono::DateTime<chrono::Utc>>,
    pub created_by: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[async_trait]
pub trait CommunicationItemsRepository: Send + Sync {
    /// フィルタ付き一覧 (件数 + ページネーション)
    async fn list(
        &self,
        tenant_id: Uuid,
        is_active: Option<bool>,
        target_employee_id: Option<Uuid>,
        per_page: i64,
        offset: i64,
    ) -> Result<(Vec<CommunicationItemWithName>, i64), sqlx::Error>;

    /// 有効期間内のアクティブ一覧
    async fn list_active(
        &self,
        tenant_id: Uuid,
        target_employee_id: Option<Uuid>,
    ) -> Result<Vec<CommunicationItemWithName>, sqlx::Error>;

    /// ID で取得
    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<CommunicationItem>, sqlx::Error>;

    /// 作成
    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateCommunicationItem,
    ) -> Result<CommunicationItem, sqlx::Error>;

    /// 更新
    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateCommunicationItem,
    ) -> Result<Option<CommunicationItem>, sqlx::Error>;

    /// 削除。Returns true if a row was affected.
    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;
}

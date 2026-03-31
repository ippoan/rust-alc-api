use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{
    CreateGuidanceRecord, GuidanceRecord, GuidanceRecordAttachment, UpdateGuidanceRecord,
};

/// list_records で使う中間型 (employee_name を JOIN で取得)
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct GuidanceRecordWithName {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub employee_id: Uuid,
    pub employee_name: Option<String>,
    pub guidance_type: String,
    pub title: String,
    pub content: String,
    pub guided_by: Option<String>,
    pub guided_at: chrono::DateTime<chrono::Utc>,
    pub parent_id: Option<Uuid>,
    pub depth: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[allow(clippy::too_many_arguments)]
#[async_trait]
pub trait GuidanceRecordsRepository: Send + Sync {
    /// トップレベルレコード数 (フィルタ付き)
    async fn count_top_level(
        &self,
        tenant_id: Uuid,
        employee_id: Option<Uuid>,
        guidance_type: Option<&str>,
        date_from: Option<&str>,
        date_to: Option<&str>,
    ) -> Result<i64, sqlx::Error>;

    /// WITH RECURSIVE でツリー取得 (トップレベルをページネーション)
    async fn list_tree(
        &self,
        tenant_id: Uuid,
        employee_id: Option<Uuid>,
        guidance_type: Option<&str>,
        date_from: Option<&str>,
        date_to: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<GuidanceRecordWithName>, sqlx::Error>;

    /// 指定レコード ID 群の添付ファイルを一括取得
    async fn list_attachments_by_record_ids(
        &self,
        tenant_id: Uuid,
        record_ids: &[Uuid],
    ) -> Result<Vec<GuidanceRecordAttachment>, sqlx::Error>;

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<GuidanceRecord>, sqlx::Error>;

    /// 親の depth を取得 (存在しない場合 None)
    async fn get_parent_depth(
        &self,
        tenant_id: Uuid,
        parent_id: Uuid,
    ) -> Result<Option<i32>, sqlx::Error>;

    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateGuidanceRecord,
        depth: i32,
    ) -> Result<GuidanceRecord, sqlx::Error>;

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateGuidanceRecord,
    ) -> Result<Option<GuidanceRecord>, sqlx::Error>;

    /// 再帰削除。削除行数を返す。
    async fn delete_recursive(&self, tenant_id: Uuid, id: Uuid) -> Result<u64, sqlx::Error>;

    /// レコードの添付ファイル一覧
    async fn list_attachments(
        &self,
        tenant_id: Uuid,
        record_id: Uuid,
    ) -> Result<Vec<GuidanceRecordAttachment>, sqlx::Error>;

    /// 添付ファイル INSERT
    async fn create_attachment(
        &self,
        tenant_id: Uuid,
        record_id: Uuid,
        file_name: &str,
        file_type: &str,
        file_size: i32,
        storage_url: &str,
    ) -> Result<GuidanceRecordAttachment, sqlx::Error>;

    /// 添付ファイル取得
    async fn get_attachment(
        &self,
        tenant_id: Uuid,
        record_id: Uuid,
        att_id: Uuid,
    ) -> Result<Option<GuidanceRecordAttachment>, sqlx::Error>;

    /// 添付ファイル削除。削除行数を返す。
    async fn delete_attachment(
        &self,
        tenant_id: Uuid,
        record_id: Uuid,
        att_id: Uuid,
    ) -> Result<u64, sqlx::Error>;
}

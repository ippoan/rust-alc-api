use async_trait::async_trait;
use uuid::Uuid;

/// 車検証ファイル (car_inspection_files_a から取得)
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct CarInspectionFile {
    pub uuid: Uuid,
    pub file_type: String,
    pub elect_cert_mg_no: String,
    pub grantdate_e: String,
    pub grantdate_y: String,
    pub grantdate_m: String,
    pub grantdate_d: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub modified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// car_inspection_files_a/b リンク作成パラメータ
pub struct CreateFileLinkParams<'a> {
    pub tenant_id: Uuid,
    pub file_uuid: Uuid,
    pub file_type: &'a str,
    pub elect_cert_mg_no: &'a str,
    pub grantdate_e: &'a str,
    pub grantdate_y: &'a str,
    pub grantdate_m: &'a str,
    pub grantdate_d: &'a str,
}

/// 車両カテゴリ集計
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct VehicleCategories {
    pub car_kinds: Vec<String>,
    pub uses: Vec<String>,
    pub car_shapes: Vec<String>,
    pub private_businesses: Vec<String>,
}

#[async_trait]
pub trait CarInspectionRepository: Send + Sync {
    /// 現在有効な車検証一覧 (DISTINCT ON CarId, to_jsonb)
    async fn list_current(&self, tenant_id: Uuid) -> Result<Vec<serde_json::Value>, sqlx::Error>;

    /// 期限切れ間近の車検証一覧
    async fn list_expired(&self, tenant_id: Uuid) -> Result<Vec<serde_json::Value>, sqlx::Error>;

    /// 更新対象の車検証一覧
    async fn list_renew(&self, tenant_id: Uuid) -> Result<Vec<serde_json::Value>, sqlx::Error>;

    /// ID で車検証取得 (to_jsonb)
    async fn get_by_id(
        &self,
        tenant_id: Uuid,
        id: i32,
    ) -> Result<Option<serde_json::Value>, sqlx::Error>;

    /// 車両カテゴリ一覧
    async fn vehicle_categories(&self, tenant_id: Uuid) -> Result<VehicleCategories, sqlx::Error>;

    /// 現在有効な車検証に紐づくファイル一覧
    async fn list_current_files(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<CarInspectionFile>, sqlx::Error>;

    /// 車検証 JSON から UPSERT (95 カラム)
    async fn upsert_from_json(
        &self,
        tenant_id: Uuid,
        cert_info: &serde_json::Value,
        cert_info_import_file_version: &str,
    ) -> Result<(), sqlx::Error>;

    /// car_inspection_files_a/b にリンクレコード挿入
    async fn create_file_link(&self, params: &CreateFileLinkParams<'_>) -> Result<(), sqlx::Error>;

    /// pending_car_inspection_pdfs から ElectCertMgNo でマッチする PDF を検索
    async fn find_pending_pdf(
        &self,
        tenant_id: Uuid,
        elect_cert_mg_no: &str,
    ) -> Result<Option<String>, sqlx::Error>;

    /// pending_car_inspection_pdfs を削除
    async fn delete_pending_pdf(
        &self,
        tenant_id: Uuid,
        elect_cert_mg_no: &str,
    ) -> Result<(), sqlx::Error>;

    /// pending_car_inspection_pdfs に UPSERT (PDF 先着時)
    async fn upsert_pending_pdf(
        &self,
        params: &CreateFileLinkParams<'_>,
    ) -> Result<(), sqlx::Error>;

    /// car_inspection_files_a に対応する JSON が存在するか確認
    async fn json_file_exists(
        &self,
        tenant_id: Uuid,
        elect_cert_mg_no: &str,
        grantdate_e: &str,
        grantdate_y: &str,
        grantdate_m: &str,
        grantdate_d: &str,
    ) -> Result<bool, sqlx::Error>;
}

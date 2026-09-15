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

/// 電子車検証の番号で car_inspection を照合した結果 (Refs ippoan/alc-app-s3#110)。
/// 照合の SQL は `crate::repo::car_inspections::lookup_expiry` の 1 か所。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarinsLookup {
    /// 2 次元コードの有効期限。形の崩れた値は None
    pub expires_on: Option<chrono::NaiveDate>,
    /// `cert_no` (管理番号で一致) / `car_id` (車両 ID で一致) / `none` (該当なし)
    pub matched_by: &'static str,
    /// 登録番号 (空なら None)。所有者・住所・車台番号は返さない
    pub car_no: Option<String>,
}

/// 番号の形が合わない (管理番号は数字 12〜13 桁、車両 ID は英数 14 桁)
#[derive(Debug, PartialEq, Eq)]
pub struct InvalidCarinsNumber;

/// 端末から受け取った電子車検証の番号を検査する。空文字は None に寄せる。
pub fn normalize_carins_numbers(
    cert_no: &mut Option<String>,
    vehicle_id: &mut Option<String>,
) -> Result<(), InvalidCarinsNumber> {
    fn check(
        value: &mut Option<String>,
        valid: fn(&str) -> bool,
    ) -> Result<(), InvalidCarinsNumber> {
        if value.as_deref() == Some("") {
            *value = None;
        }
        match value.as_deref() {
            Some(v) if !valid(v) => Err(InvalidCarinsNumber),
            _ => Ok(()),
        }
    }
    check(cert_no, |v| {
        (12..=13).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_digit())
    })?;
    check(vehicle_id, |v| {
        v.len() == 14 && v.bytes().all(|b| b.is_ascii_alphanumeric())
    })
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

    /// CarId で同一車両の車検証履歴一覧 (新→旧順、pdfUuid/jsonUuid 付き)
    async fn list_by_car_id(
        &self,
        tenant_id: Uuid,
        car_id: &str,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error>;

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

    /// 電子車検証の管理番号 / 車両 ID で期限を照合する (kiosk の照合口)
    async fn lookup_expiry(
        &self,
        tenant_id: Uuid,
        cert_no: Option<&str>,
        car_id: Option<&str>,
    ) -> Result<CarinsLookup, sqlx::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normalize(
        cert_no: Option<&str>,
        vehicle_id: Option<&str>,
    ) -> Result<(Option<String>, Option<String>), InvalidCarinsNumber> {
        let mut c = cert_no.map(str::to_string);
        let mut v = vehicle_id.map(str::to_string);
        normalize_carins_numbers(&mut c, &mut v).map(|()| (c, v))
    }

    #[test]
    fn test_normalize_carins_numbers_accepts_valid_shapes() {
        assert_eq!(normalize(None, None), Ok((None, None)));
        assert_eq!(
            normalize(Some("000000000001"), Some("TESTCARID00001")),
            Ok((
                Some("000000000001".to_string()),
                Some("TESTCARID00001".to_string())
            ))
        );
        assert!(normalize(Some("0000000000001"), None).is_ok());
    }

    #[test]
    fn test_normalize_carins_numbers_empty_is_none() {
        assert_eq!(normalize(Some(""), Some("")), Ok((None, None)));
    }

    #[test]
    fn test_normalize_carins_numbers_rejects_bad_shapes() {
        assert!(normalize(Some("00000000001"), None).is_err());
        assert!(normalize(Some("00000000000012"), None).is_err());
        assert!(normalize(Some("00000000000A"), None).is_err());
        assert!(normalize(None, Some("TESTCARID0001")).is_err());
        assert!(normalize(None, Some("TESTCARID-0001")).is_err());
    }
}

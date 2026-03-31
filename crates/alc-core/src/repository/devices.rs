use async_trait::async_trait;
use uuid::Uuid;

// ============================================================
// Repository 用の型定義
// ============================================================

/// デバイス情報 (list_devices / get 用)
#[derive(Debug, sqlx::FromRow)]
pub struct DeviceRow {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub device_name: String,
    pub device_type: String,
    pub phone_number: Option<String>,
    pub user_id: Option<Uuid>,
    pub status: String,
    pub approved_by: Option<Uuid>,
    pub approved_at: Option<String>,
    pub last_seen_at: Option<String>,
    pub call_enabled: bool,
    pub call_schedule: Option<serde_json::Value>,
    pub fcm_token: Option<String>,
    pub last_login_employee_id: Option<Uuid>,
    pub last_login_employee_name: Option<String>,
    pub last_login_employee_role: Option<Vec<String>>,
    pub app_version_code: Option<i32>,
    pub app_version_name: Option<String>,
    pub is_device_owner: bool,
    pub is_dev_device: bool,
    pub always_on: bool,
    pub watchdog_running: Option<bool>,
    pub created_at: String,
    pub updated_at: String,
}

/// 登録リクエスト情報
#[derive(Debug, sqlx::FromRow)]
pub struct RegistrationRequestRow {
    pub id: Uuid,
    pub registration_code: String,
    pub flow_type: String,
    pub tenant_id: Option<Uuid>,
    pub phone_number: Option<String>,
    pub device_name: String,
    pub status: String,
    pub device_id: Option<Uuid>,
    pub expires_at: Option<String>,
    pub is_device_owner: bool,
    pub is_dev_device: bool,
    pub created_at: String,
}

/// 登録リクエスト作成結果
pub struct CreateRegistrationResult {
    pub registration_code: String,
    pub expires_at: String,
}

/// ステータス確認結果
#[derive(sqlx::FromRow)]
pub struct RegistrationStatusRow {
    pub status: String,
    pub device_id: Option<Uuid>,
    pub tenant_id: Option<Uuid>,
    pub expires_at: Option<String>,
    pub device_name: Option<String>,
}

/// claim 検索結果
#[derive(sqlx::FromRow)]
pub struct ClaimLookupRow {
    pub id: Uuid,
    pub flow_type: String,
    pub tenant_id: Option<Uuid>,
    pub status: String,
    pub expires_at: Option<String>,
    pub device_name: Option<String>,
    pub is_device_owner: bool,
    pub is_dev_device: bool,
}

/// approve 検索結果
#[derive(sqlx::FromRow)]
pub struct ApproveLookupRow {
    pub id: Uuid,
    pub flow_type: String,
    pub phone_number: Option<String>,
    pub device_name: Option<String>,
    pub status: String,
    pub is_device_owner: bool,
    pub is_dev_device: bool,
}

/// デバイス設定取得結果
#[derive(Debug, sqlx::FromRow)]
pub struct DeviceSettingsRow {
    pub call_enabled: bool,
    pub call_schedule: Option<serde_json::Value>,
    pub status: String,
    pub last_login_employee_id: Option<Uuid>,
    pub last_login_employee_name: Option<String>,
    pub last_login_employee_role: Option<Vec<String>>,
    pub always_on: bool,
}

/// FCM デバイス情報
#[derive(Debug, sqlx::FromRow)]
pub struct FcmDeviceRow {
    pub id: Uuid,
    pub fcm_token: String,
    pub call_enabled: bool,
    pub call_schedule: Option<serde_json::Value>,
}

/// OTA 対象デバイス情報
#[derive(Debug, sqlx::FromRow)]
pub struct OtaDeviceRow {
    pub id: Uuid,
    pub device_name: String,
    pub fcm_token: String,
    pub app_version_code: Option<i32>,
}

/// テナント付き FCM トークン情報
#[derive(Debug, sqlx::FromRow)]
pub struct DeviceTenantRow {
    pub tenant_id: Uuid,
}

/// FCM トークン付きデバイス (テスト送信用)
#[derive(Debug, sqlx::FromRow)]
pub struct FcmTestDeviceRow {
    pub id: Uuid,
    pub device_name: String,
    pub fcm_token: String,
}

// ============================================================
// Trait
// ============================================================

#[allow(clippy::too_many_arguments)]
#[async_trait]
pub trait DeviceRepository: Send + Sync {
    // --- Public (no tenant context) ---

    /// 6桁コードの存在チェック
    async fn code_exists(&self, code: &str) -> Result<bool, sqlx::Error>;

    /// QR一時登録リクエスト作成
    async fn create_registration_request(
        &self,
        code: &str,
        device_name: &str,
    ) -> Result<CreateRegistrationResult, sqlx::Error>;

    /// 登録リクエストのステータス確認
    async fn get_registration_status(
        &self,
        code: &str,
    ) -> Result<Option<RegistrationStatusRow>, sqlx::Error>;

    /// 期限切れチェック (timestamptz < NOW())
    async fn is_expired(&self, expires_at: &str) -> Result<bool, sqlx::Error>;

    /// claim: 登録リクエスト検索
    async fn find_claim_request(&self, code: &str) -> Result<Option<ClaimLookupRow>, sqlx::Error>;

    /// claim: URL/device_owner フロー - デバイス作成 + リクエスト更新 (トランザクション)
    async fn claim_url_flow(
        &self,
        tenant_id: Uuid,
        device_name: &str,
        phone_number: Option<&str>,
        is_device_owner: bool,
        is_dev_device: bool,
        req_id: Uuid,
    ) -> Result<Uuid, sqlx::Error>;

    /// claim: QR永久 - phone_number/device_name 更新
    async fn claim_update_permanent_qr(
        &self,
        req_id: Uuid,
        phone_number: Option<&str>,
        device_name: &str,
    ) -> Result<(), sqlx::Error>;

    /// デバイス設定取得 (認証不要、SECURITY DEFINER 関数経由)
    async fn get_device_settings(
        &self,
        device_id: Uuid,
    ) -> Result<Option<DeviceSettingsRow>, sqlx::Error>;

    /// device_id からテナント ID を検索
    async fn lookup_device_tenant(&self, device_id: Uuid) -> Result<Option<Uuid>, sqlx::Error>;

    /// FCM トークン登録
    async fn update_fcm_token(
        &self,
        device_id: Uuid,
        tenant_id: Uuid,
        fcm_token: &str,
    ) -> Result<(), sqlx::Error>;

    /// 最終ログインユーザー更新
    async fn update_last_login(
        &self,
        device_id: Uuid,
        tenant_id: Uuid,
        employee_id: Uuid,
        employee_name: &str,
        employee_role: &[String],
    ) -> Result<(), sqlx::Error>;

    /// アクティブかつ FCM トークンありのデバイス一覧
    async fn list_fcm_devices(&self) -> Result<Vec<FcmDeviceRow>, sqlx::Error>;

    /// device_id からテナント ID + ステータス確認 (FCM dismiss 用)
    async fn get_device_tenant_active(
        &self,
        device_id: Uuid,
    ) -> Result<Option<DeviceTenantRow>, sqlx::Error>;

    /// 同一テナントの他デバイスの FCM トークン一覧 (dismiss 用)
    async fn list_tenant_fcm_tokens_except(
        &self,
        tenant_id: Uuid,
        exclude_device_id: Uuid,
    ) -> Result<Vec<String>, sqlx::Error>;

    /// 全テナントのアクティブ + FCM + call_enabled デバイス (test_fcm_all_exclude 用)
    async fn list_all_callable_devices(&self) -> Result<Vec<FcmTestDeviceRow>, sqlx::Error>;

    /// Watchdog 状態報告
    async fn update_watchdog_state(
        &self,
        device_id: Uuid,
        tenant_id: Uuid,
        running: bool,
    ) -> Result<(), sqlx::Error>;

    /// バージョン報告
    async fn report_version(
        &self,
        device_id: Uuid,
        tenant_id: Uuid,
        version_code: i32,
        version_name: &str,
        is_device_owner: bool,
        is_dev_device: bool,
    ) -> Result<(), sqlx::Error>;

    /// 全テナントの dev 端末がある tenant_id 一覧
    async fn list_dev_device_tenant_ids(&self) -> Result<Vec<String>, sqlx::Error>;

    // --- Tenant-scoped ---

    /// デバイス一覧
    async fn list_devices(&self, tenant_id: Uuid) -> Result<Vec<DeviceRow>, sqlx::Error>;

    /// 承認待ちリクエスト一覧
    async fn list_pending(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<RegistrationRequestRow>, sqlx::Error>;

    /// URL トークン生成
    async fn create_url_token(
        &self,
        tenant_id: Uuid,
        code: &str,
        device_name: &str,
        is_device_owner: bool,
        is_dev_device: bool,
    ) -> Result<(), sqlx::Error>;

    /// Device Owner トークン生成
    async fn create_device_owner_token(
        &self,
        tenant_id: Uuid,
        code: &str,
        device_name: &str,
        is_dev_device: bool,
    ) -> Result<(), sqlx::Error>;

    /// QR 永久コード生成
    async fn create_permanent_qr(
        &self,
        tenant_id: Uuid,
        code: &str,
        device_name: &str,
        is_device_owner: bool,
        is_dev_device: bool,
    ) -> Result<(), sqlx::Error>;

    /// 承認: リクエスト検索 (tenant-scoped tx 内)
    async fn find_approve_request(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<ApproveLookupRow>, sqlx::Error>;

    /// 承認: デバイス作成 + リクエスト更新 (トランザクション)
    async fn approve_device(
        &self,
        tenant_id: Uuid,
        req_id: Uuid,
        device_name: &str,
        device_type: &str,
        phone_number: Option<&str>,
        approved_by: Option<Uuid>,
        is_device_owner: bool,
        is_dev_device: bool,
    ) -> Result<Uuid, sqlx::Error>;

    /// コードで承認: リクエスト検索
    async fn find_approve_by_code_request(
        &self,
        tenant_id: Uuid,
        code: &str,
    ) -> Result<Option<ApproveLookupRow>, sqlx::Error>;

    /// コードで承認: デバイス作成 + リクエスト更新 (トランザクション)
    async fn approve_by_code(
        &self,
        tenant_id: Uuid,
        req_id: Uuid,
        device_name: &str,
        device_type: &str,
        phone_number: Option<&str>,
        approved_by: Option<Uuid>,
        is_device_owner: bool,
        is_dev_device: bool,
    ) -> Result<Uuid, sqlx::Error>;

    /// 拒否
    async fn reject_device(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;

    /// 無効化
    async fn disable_device(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;

    /// 有効化
    async fn enable_device(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;

    /// 削除
    async fn delete_device(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;

    /// 着信設定更新
    async fn update_call_settings(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        call_enabled: bool,
        call_schedule: Option<&serde_json::Value>,
        always_on: Option<bool>,
    ) -> Result<bool, sqlx::Error>;

    /// FCM トークン取得 (RLS 回避、pool 直接)
    async fn get_fcm_token_bypass_rls(
        &self,
        device_id: Uuid,
    ) -> Result<Option<Option<String>>, sqlx::Error>;

    /// FCM テスト用: デバイスの FCM トークン取得 (tenant-scoped)
    async fn get_device_fcm_token(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Option<String>>, sqlx::Error>;

    /// FCM 一括テスト用: テナント内のアクティブ + FCM トークンありデバイス
    async fn list_tenant_fcm_devices(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<FcmTestDeviceRow>, sqlx::Error>;

    /// OTA: テナント内デバイス一覧 (dev_only フィルタ対応)
    async fn list_ota_devices(
        &self,
        tenant_id: Uuid,
        dev_only: bool,
    ) -> Result<Vec<OtaDeviceRow>, sqlx::Error>;
}

use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

use rust_alc_api::db::models::DtakologRow;
use rust_alc_api::db::models::*;
use rust_alc_api::db::repository::auth::{AuthRepository, SsoConfigRow};
use rust_alc_api::db::repository::bot_admin::{BotAdminRepository, BotConfigRow};
use rust_alc_api::db::repository::car_inspections::{
    CarInspectionFile, CarInspectionRepository, VehicleCategories,
};
use rust_alc_api::db::repository::carins_files::CarinsFilesRepository;
use rust_alc_api::db::repository::carins_files::FileRow;
use rust_alc_api::db::repository::carrying_items::CarryingItemsRepository;
use rust_alc_api::db::repository::communication_items::{
    CommunicationItemWithName, CommunicationItemsRepository,
};
use rust_alc_api::db::repository::daily_health::DailyHealthRepository;
use rust_alc_api::db::repository::daily_health::DailyHealthRow;
use rust_alc_api::db::repository::devices::{
    ApproveLookupRow, ClaimLookupRow, CreateRegistrationResult, DeviceRepository, DeviceRow,
    DeviceSettingsRow, DeviceTenantRow, FcmDeviceRow, FcmTestDeviceRow, OtaDeviceRow,
    RegistrationRequestRow, RegistrationStatusRow,
};
use rust_alc_api::db::repository::driver_info::DriverInfoRepository;
use rust_alc_api::db::repository::driver_info::{
    DailyInspectionSummary, InstructionSummary, MeasurementSummary,
};
use rust_alc_api::db::repository::dtako_csv_proxy::DtakoCsvProxyRepository;
use rust_alc_api::db::repository::dtako_daily_hours::DtakoDailyHoursRepository;
use rust_alc_api::db::repository::dtako_drivers::{Driver, DtakoDriversRepository};
use rust_alc_api::db::repository::dtako_logs::DtakoLogsRepository;

macro_rules! check_fail {
    ($self:expr) => {
        if $self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
    };
}

// ============================================================
// MockAuthRepository
// ============================================================

/// Helper to create a dummy User for tests
pub fn mock_user(tenant_id: Uuid) -> User {
    User {
        id: Uuid::new_v4(),
        tenant_id,
        google_sub: Some("test-google-sub-12345".to_string()),
        lineworks_id: None,
        email: "google-test@example.com".to_string(),
        name: "Google Test User".to_string(),
        role: "admin".to_string(),
        refresh_token_hash: None,
        refresh_token_expires_at: None,
        created_at: Utc::now(),
    }
}

pub struct MockAuthRepository {
    pub fail_next: AtomicBool,
    /// If Some, find_user_by_google_sub returns this user
    pub return_user: std::sync::Mutex<Option<User>>,
    /// If Some, find_user_by_refresh_token_hash returns this user
    pub return_refresh_user: std::sync::Mutex<Option<User>>,
    /// If Some, find_invitation_by_email returns this invitation
    pub return_invitation: std::sync::Mutex<Option<TenantAllowedEmail>>,
    /// If Some, find_tenant_by_email_domain returns this tenant
    pub return_domain_tenant: std::sync::Mutex<Option<Tenant>>,
    /// If Some, get_tenant_by_id returns this tenant
    pub return_tenant: std::sync::Mutex<Option<Tenant>>,
    /// If Some, resolve_sso_config returns this config
    pub return_sso_config: std::sync::Mutex<Option<SsoConfigRow>>,
    /// Tenant ID to use for create_tenant_with_domain / create_tenant_by_name
    pub auto_tenant_id: std::sync::Mutex<Option<Uuid>>,
    /// If Some, find_user_by_lineworks_id returns this user
    pub return_lineworks_user: std::sync::Mutex<Option<User>>,
    /// If true, create_user_lineworks returns error
    pub fail_on_create_user: AtomicBool,
}

impl Default for MockAuthRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            return_user: std::sync::Mutex::new(None),
            return_refresh_user: std::sync::Mutex::new(None),
            return_invitation: std::sync::Mutex::new(None),
            return_domain_tenant: std::sync::Mutex::new(None),
            return_tenant: std::sync::Mutex::new(None),
            return_sso_config: std::sync::Mutex::new(None),
            auto_tenant_id: std::sync::Mutex::new(None),
            return_lineworks_user: std::sync::Mutex::new(None),
            fail_on_create_user: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl AuthRepository for MockAuthRepository {
    async fn find_user_by_google_sub(
        &self,
        _google_sub: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_user.lock().unwrap().clone())
    }

    async fn find_user_by_lineworks_id(
        &self,
        _lineworks_id: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_lineworks_user.lock().unwrap().clone())
    }

    async fn find_user_by_refresh_token_hash(
        &self,
        _token_hash: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_refresh_user.lock().unwrap().clone())
    }

    async fn find_invitation_by_email(
        &self,
        _email: &str,
    ) -> Result<Option<TenantAllowedEmail>, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_invitation.lock().unwrap().clone())
    }

    async fn delete_invitation(&self, _id: Uuid) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn find_tenant_by_email_domain(
        &self,
        _email_domain: &str,
    ) -> Result<Option<Tenant>, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_domain_tenant.lock().unwrap().clone())
    }

    async fn create_tenant_with_domain(&self, email_domain: &str) -> Result<Tenant, sqlx::Error> {
        check_fail!(self);
        let tid = self
            .auto_tenant_id
            .lock()
            .unwrap()
            .unwrap_or_else(Uuid::new_v4);
        Ok(Tenant {
            id: tid,
            name: email_domain.to_string(),
            slug: None,
            email_domain: Some(email_domain.to_string()),
            created_at: Utc::now(),
        })
    }

    async fn create_tenant_by_name(&self, name: &str) -> Result<Tenant, sqlx::Error> {
        check_fail!(self);
        let tid = self
            .auto_tenant_id
            .lock()
            .unwrap()
            .unwrap_or_else(Uuid::new_v4);
        Ok(Tenant {
            id: tid,
            name: name.to_string(),
            slug: None,
            email_domain: None,
            created_at: Utc::now(),
        })
    }

    async fn get_tenant_by_id(&self, _id: Uuid) -> Result<Option<Tenant>, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_tenant.lock().unwrap().clone())
    }

    async fn get_tenant_slug(&self, _tenant_id: Uuid) -> Result<Option<String>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn create_user_google(
        &self,
        tenant_id: Uuid,
        google_sub: &str,
        email: &str,
        name: &str,
        role: &str,
    ) -> Result<User, sqlx::Error> {
        check_fail!(self);
        Ok(User {
            id: Uuid::new_v4(),
            tenant_id,
            google_sub: Some(google_sub.to_string()),
            lineworks_id: None,
            email: email.to_string(),
            name: name.to_string(),
            role: role.to_string(),
            refresh_token_hash: None,
            refresh_token_expires_at: None,
            created_at: Utc::now(),
        })
    }

    async fn create_user_lineworks(
        &self,
        tenant_id: Uuid,
        lineworks_id: &str,
        email: &str,
        name: &str,
    ) -> Result<User, sqlx::Error> {
        check_fail!(self);
        if self.fail_on_create_user.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(User {
            id: Uuid::new_v4(),
            tenant_id,
            google_sub: None,
            lineworks_id: Some(lineworks_id.to_string()),
            email: email.to_string(),
            name: name.to_string(),
            role: "admin".to_string(),
            refresh_token_hash: None,
            refresh_token_expires_at: None,
            created_at: Utc::now(),
        })
    }

    async fn save_refresh_token(
        &self,
        _user_id: Uuid,
        _refresh_hash: &str,
        _expires_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn clear_refresh_token(&self, _user_id: Uuid) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn resolve_sso_config(
        &self,
        _provider: &str,
        _domain: &str,
    ) -> Result<Option<SsoConfigRow>, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_sso_config.lock().unwrap().clone())
    }

    async fn resolve_sso_config_required(
        &self,
        _provider: &str,
        _domain: &str,
    ) -> Result<SsoConfigRow, sqlx::Error> {
        check_fail!(self);
        self.return_sso_config
            .lock()
            .unwrap()
            .clone()
            .ok_or(sqlx::Error::RowNotFound)
    }
}

// ============================================================
// MockBotAdminRepository
// ============================================================

pub struct MockBotAdminRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockBotAdminRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl BotAdminRepository for MockBotAdminRepository {
    async fn list_configs(&self, _tenant_id: Uuid) -> Result<Vec<BotConfigRow>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn update_client_secret(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _encrypted: &str,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn update_private_key(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _encrypted: &str,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn update_config(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _provider: &str,
        _name: &str,
        _client_id: &str,
        _service_account: &str,
        _bot_id: &str,
        _enabled: bool,
    ) -> Result<BotConfigRow, sqlx::Error> {
        check_fail!(self);
        Ok(BotConfigRow {
            id: _id,
            provider: _provider.to_string(),
            name: _name.to_string(),
            client_id: _client_id.to_string(),
            service_account: _service_account.to_string(),
            bot_id: _bot_id.to_string(),
            enabled: _enabled,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
    }

    async fn create_config(
        &self,
        _tenant_id: Uuid,
        _provider: &str,
        _name: &str,
        _client_id: &str,
        _client_secret_encrypted: &str,
        _service_account: &str,
        _private_key_encrypted: &str,
        _bot_id: &str,
        _enabled: bool,
    ) -> Result<BotConfigRow, sqlx::Error> {
        check_fail!(self);
        Ok(BotConfigRow {
            id: Uuid::new_v4(),
            provider: _provider.to_string(),
            name: _name.to_string(),
            client_id: _client_id.to_string(),
            service_account: _service_account.to_string(),
            bot_id: _bot_id.to_string(),
            enabled: _enabled,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
    }

    async fn delete_config(&self, _tenant_id: Uuid, _id: Uuid) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }
}

// ============================================================
// MockCarInspectionRepository
// ============================================================

pub struct MockCarInspectionRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockCarInspectionRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl CarInspectionRepository for MockCarInspectionRepository {
    async fn list_current(&self, _tenant_id: Uuid) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn list_expired(&self, _tenant_id: Uuid) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn list_renew(&self, _tenant_id: Uuid) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn get_by_id(
        &self,
        _tenant_id: Uuid,
        _id: i32,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn vehicle_categories(&self, _tenant_id: Uuid) -> Result<VehicleCategories, sqlx::Error> {
        check_fail!(self);
        Ok(VehicleCategories {
            car_kinds: vec![],
            uses: vec![],
            car_shapes: vec![],
            private_businesses: vec![],
        })
    }

    async fn list_current_files(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<CarInspectionFile>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn upsert_from_json(
        &self,
        _tenant_id: Uuid,
        _cert_info: &serde_json::Value,
        _cert_info_import_file_version: &str,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn create_file_link(
        &self,
        _params: &alc_core::repository::car_inspections::CreateFileLinkParams<'_>,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn find_pending_pdf(
        &self,
        _tenant_id: Uuid,
        _elect_cert_mg_no: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn delete_pending_pdf(
        &self,
        _tenant_id: Uuid,
        _elect_cert_mg_no: &str,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn upsert_pending_pdf(
        &self,
        _params: &alc_core::repository::car_inspections::CreateFileLinkParams<'_>,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn json_file_exists(
        &self,
        _tenant_id: Uuid,
        _elect_cert_mg_no: &str,
        _grantdate_e: &str,
        _grantdate_y: &str,
        _grantdate_m: &str,
        _grantdate_d: &str,
    ) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(false)
    }
}

// ============================================================
// MockCarinsFilesRepository
// ============================================================

pub struct MockCarinsFilesRepository {
    pub fail_next: AtomicBool,
    pub return_file: std::sync::Mutex<Option<FileRow>>,
    pub return_affected: std::sync::Mutex<bool>,
}

impl Default for MockCarinsFilesRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            return_file: std::sync::Mutex::new(None),
            return_affected: std::sync::Mutex::new(false),
        }
    }
}

#[async_trait::async_trait]
impl CarinsFilesRepository for MockCarinsFilesRepository {
    async fn list_files(
        &self,
        _tenant_id: Uuid,
        _type_filter: Option<&str>,
    ) -> Result<Vec<FileRow>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn list_recent(&self, _tenant_id: Uuid) -> Result<Vec<FileRow>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn list_not_attached(&self, _tenant_id: Uuid) -> Result<Vec<FileRow>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn get_file(
        &self,
        _tenant_id: Uuid,
        _uuid: &str,
    ) -> Result<Option<FileRow>, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_file.lock().unwrap().clone())
    }

    async fn get_file_for_download(
        &self,
        _tenant_id: Uuid,
        _uuid: &str,
    ) -> Result<Option<FileRow>, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_file.lock().unwrap().clone())
    }

    async fn create_file(
        &self,
        _tenant_id: Uuid,
        _file_uuid: Uuid,
        _filename: &str,
        _file_type: &str,
        _gcs_key: &str,
        _now: DateTime<Utc>,
    ) -> Result<FileRow, sqlx::Error> {
        check_fail!(self);
        Ok(FileRow {
            uuid: _file_uuid.to_string(),
            filename: _filename.to_string(),
            file_type: _file_type.to_string(),
            created: _now.to_rfc3339(),
            deleted: None,
            blob: None,
            s3_key: Some(_gcs_key.to_string()),
            storage_class: Some("STANDARD".to_string()),
            last_accessed_at: None,
            access_count_weekly: None,
            access_count_total: None,
            promoted_to_standard_at: None,
        })
    }

    async fn delete_file(&self, _tenant_id: Uuid, _uuid: &str) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(*self.return_affected.lock().unwrap())
    }

    async fn restore_file(&self, _tenant_id: Uuid, _uuid: &str) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(*self.return_affected.lock().unwrap())
    }
}

// ============================================================
// MockCarryingItemsRepository
// ============================================================

pub struct MockCarryingItemsRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockCarryingItemsRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl CarryingItemsRepository for MockCarryingItemsRepository {
    async fn list(&self, _tenant_id: Uuid) -> Result<Vec<CarryingItem>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn list_conditions(
        &self,
        _tenant_id: Uuid,
        _item_ids: &[Uuid],
    ) -> Result<Vec<CarryingItemVehicleCondition>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn create(
        &self,
        _tenant_id: Uuid,
        _item_name: &str,
        _is_required: bool,
        _sort_order: i32,
    ) -> Result<CarryingItem, sqlx::Error> {
        check_fail!(self);
        todo!("MockCarryingItemsRepository::create")
    }

    async fn insert_condition(
        &self,
        _tenant_id: Uuid,
        _item_id: Uuid,
        _category: &str,
        _value: &str,
    ) -> Result<Option<CarryingItemVehicleCondition>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn update(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _item_name: Option<&str>,
        _is_required: Option<bool>,
        _sort_order: Option<i32>,
    ) -> Result<Option<CarryingItem>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn delete_conditions(&self, _tenant_id: Uuid, _item_id: Uuid) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn get_conditions(
        &self,
        _tenant_id: Uuid,
        _item_id: Uuid,
    ) -> Result<Vec<CarryingItemVehicleCondition>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn delete(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(false)
    }
}

// ============================================================
// MockCommunicationItemsRepository
// ============================================================

pub struct MockCommunicationItemsRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockCommunicationItemsRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl CommunicationItemsRepository for MockCommunicationItemsRepository {
    async fn list(
        &self,
        _tenant_id: Uuid,
        _is_active: Option<bool>,
        _target_employee_id: Option<Uuid>,
        _per_page: i64,
        _offset: i64,
    ) -> Result<(Vec<CommunicationItemWithName>, i64), sqlx::Error> {
        check_fail!(self);
        Ok((vec![], 0))
    }

    async fn list_active(
        &self,
        _tenant_id: Uuid,
        _target_employee_id: Option<Uuid>,
    ) -> Result<Vec<CommunicationItemWithName>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn get(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<CommunicationItem>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn create(
        &self,
        _tenant_id: Uuid,
        _input: &CreateCommunicationItem,
    ) -> Result<CommunicationItem, sqlx::Error> {
        check_fail!(self);
        Ok(CommunicationItem {
            id: Uuid::new_v4(),
            tenant_id: _tenant_id,
            title: _input.title.clone(),
            content: _input.content.clone().unwrap_or_default(),
            priority: _input
                .priority
                .clone()
                .unwrap_or_else(|| "normal".to_string()),
            target_employee_id: _input.target_employee_id,
            is_active: true,
            effective_from: _input.effective_from,
            effective_until: _input.effective_until,
            created_by: _input.created_by.clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    async fn update(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _input: &UpdateCommunicationItem,
    ) -> Result<Option<CommunicationItem>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn delete(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(false)
    }
}

// ============================================================
// MockDailyHealthRepository
// ============================================================

pub struct MockDailyHealthRepository {
    pub fail_next: AtomicBool,
    pub data: std::sync::Mutex<Vec<DailyHealthRow>>,
}

impl Default for MockDailyHealthRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            data: std::sync::Mutex::new(vec![]),
        }
    }
}

#[async_trait::async_trait]
impl DailyHealthRepository for MockDailyHealthRepository {
    async fn fetch_daily_health(
        &self,
        _tenant_id: Uuid,
        _date: NaiveDate,
    ) -> Result<Vec<DailyHealthRow>, sqlx::Error> {
        check_fail!(self);
        Ok(self.data.lock().unwrap().clone())
    }
}

// ============================================================
// MockDeviceRepository
// ============================================================

pub struct MockDeviceRepository {
    pub fail_next: AtomicBool,
    pub return_data: AtomicBool,
    /// claim で qr_permanent フローを返す
    pub return_qr_permanent: AtomicBool,
    /// claim で status="used" を返す
    pub return_used_status: AtomicBool,
    /// claim で未知のフロータイプを返す
    pub return_unknown_flow: AtomicBool,
    /// check_registration_status で status="approved" (非 pending) を返す
    pub return_approved_status: AtomicBool,
    /// check_registration_status で expires_at=None を返す
    pub return_no_expires: AtomicBool,
    /// is_expired で true を返す
    pub return_expired: AtomicBool,
    /// list_fcm_devices で call_enabled=false のデバイスを返す
    pub return_call_disabled: AtomicBool,
    /// list_fcm_devices で call_schedule 付きデバイスを返す (enabled=false)
    pub return_schedule_disabled: AtomicBool,
    /// list_fcm_devices で日曜のみ schedule を返す (time-based test に使う)
    pub return_schedule_with_days: AtomicBool,
    /// get_fcm_token_bypass_rls で None (token なし) を返す
    pub return_no_fcm_token: AtomicBool,
    /// get_device_fcm_token で Some(None) (token null) を返す
    pub return_null_fcm_token: AtomicBool,
    /// list_all_callable_devices でデバイスを返す
    pub return_callable_devices: AtomicBool,
    /// list_dev_device_tenant_ids でテナント ID を返す
    pub return_dev_tenants: AtomicBool,
    /// list_ota_devices で version_code=None のデバイスを返す
    pub return_ota_no_version: AtomicBool,
    /// list_devices で DeviceRow を返す (From テスト用)
    pub return_device_rows: AtomicBool,
    /// list_pending で RegistrationRequestRow を返す (From テスト用)
    pub return_pending_rows: AtomicBool,
    /// approve_device / approve_by_code で失敗する (lookup は成功、create で失敗)
    pub fail_on_approve: AtomicBool,
    /// update 系メソッドで失敗する (lookup は成功、update で失敗)
    pub fail_on_update: AtomicBool,
    /// list_tenant_fcm_tokens_except でトークンを返す
    pub return_fcm_tokens: AtomicBool,
    /// list_fcm_devices で days に今日を含まないスケジュールを返す
    pub return_schedule_days_mismatch: AtomicBool,
    /// list_fcm_devices で深夜跨ぎスケジュールを返す (22:00-06:00)
    pub return_schedule_overnight: AtomicBool,
    /// code_exists で初回だけ true を返す (コード衝突リトライテスト用)
    pub return_code_exists_once: AtomicBool,
}

impl Default for MockDeviceRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            return_data: AtomicBool::new(false),
            return_qr_permanent: AtomicBool::new(false),
            return_used_status: AtomicBool::new(false),
            return_unknown_flow: AtomicBool::new(false),
            return_approved_status: AtomicBool::new(false),
            return_no_expires: AtomicBool::new(false),
            return_expired: AtomicBool::new(false),
            return_call_disabled: AtomicBool::new(false),
            return_schedule_disabled: AtomicBool::new(false),
            return_schedule_with_days: AtomicBool::new(false),
            return_no_fcm_token: AtomicBool::new(false),
            return_null_fcm_token: AtomicBool::new(false),
            return_callable_devices: AtomicBool::new(false),
            return_dev_tenants: AtomicBool::new(false),
            return_ota_no_version: AtomicBool::new(false),
            return_device_rows: AtomicBool::new(false),
            return_pending_rows: AtomicBool::new(false),
            fail_on_approve: AtomicBool::new(false),
            fail_on_update: AtomicBool::new(false),
            return_fcm_tokens: AtomicBool::new(false),
            return_schedule_days_mismatch: AtomicBool::new(false),
            return_schedule_overnight: AtomicBool::new(false),
            return_code_exists_once: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl DeviceRepository for MockDeviceRepository {
    // --- Public (no tenant context) ---

    async fn code_exists(&self, _code: &str) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        if self.return_code_exists_once.swap(false, Ordering::Relaxed) {
            return Ok(true);
        }
        Ok(false)
    }

    async fn create_registration_request(
        &self,
        _code: &str,
        _device_name: &str,
    ) -> Result<CreateRegistrationResult, sqlx::Error> {
        check_fail!(self);
        Ok(CreateRegistrationResult {
            registration_code: _code.to_string(),
            expires_at: "2026-12-31T23:59:59Z".to_string(),
        })
    }

    async fn get_registration_status(
        &self,
        _code: &str,
    ) -> Result<Option<RegistrationStatusRow>, sqlx::Error> {
        check_fail!(self);
        if self.return_data.load(Ordering::SeqCst) {
            let status = if self.return_approved_status.load(Ordering::SeqCst) {
                "approved".to_string()
            } else {
                "pending".to_string()
            };
            let expires_at = if self.return_no_expires.load(Ordering::SeqCst) {
                None
            } else {
                Some("2099-12-31T23:59:59Z".to_string())
            };
            Ok(Some(RegistrationStatusRow {
                status,
                device_id: None,
                tenant_id: Some(Uuid::nil()),
                expires_at,
                device_name: Some("Test Device".to_string()),
            }))
        } else {
            Ok(None)
        }
    }

    async fn is_expired(&self, _expires_at: &str) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_expired.load(Ordering::SeqCst))
    }

    async fn find_claim_request(&self, _code: &str) -> Result<Option<ClaimLookupRow>, sqlx::Error> {
        check_fail!(self);
        if self.return_data.load(Ordering::SeqCst) {
            let flow_type = if self.return_qr_permanent.load(Ordering::SeqCst) {
                "qr_permanent".to_string()
            } else if self.return_unknown_flow.load(Ordering::SeqCst) {
                "unknown_flow".to_string()
            } else {
                "url".to_string()
            };
            let status = if self.return_used_status.load(Ordering::SeqCst) {
                "used".to_string()
            } else {
                "pending".to_string()
            };
            Ok(Some(ClaimLookupRow {
                id: Uuid::nil(),
                flow_type,
                tenant_id: Some(Uuid::nil()),
                status,
                expires_at: Some("2099-12-31T23:59:59Z".to_string()),
                device_name: Some("Test Device".to_string()),
                is_device_owner: false,
                is_dev_device: false,
            }))
        } else {
            Ok(None)
        }
    }

    async fn claim_url_flow(
        &self,
        _tenant_id: Uuid,
        _device_name: &str,
        _phone_number: Option<&str>,
        _is_device_owner: bool,
        _is_dev_device: bool,
        _req_id: Uuid,
    ) -> Result<Uuid, sqlx::Error> {
        check_fail!(self);
        Ok(Uuid::nil())
    }

    async fn claim_update_permanent_qr(
        &self,
        _req_id: Uuid,
        _phone_number: Option<&str>,
        _device_name: &str,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn get_device_settings(
        &self,
        _device_id: Uuid,
    ) -> Result<Option<DeviceSettingsRow>, sqlx::Error> {
        check_fail!(self);
        if self.return_data.load(Ordering::SeqCst) {
            Ok(Some(DeviceSettingsRow {
                call_enabled: true,
                call_schedule: None,
                status: "active".to_string(),
                last_login_employee_id: None,
                last_login_employee_name: None,
                last_login_employee_role: None,
                always_on: false,
            }))
        } else {
            Ok(None)
        }
    }

    async fn lookup_device_tenant(&self, _device_id: Uuid) -> Result<Option<Uuid>, sqlx::Error> {
        check_fail!(self);
        if self.return_data.load(Ordering::SeqCst) {
            Ok(Some(Uuid::nil()))
        } else {
            Ok(None)
        }
    }

    async fn update_fcm_token(
        &self,
        _device_id: Uuid,
        _tenant_id: Uuid,
        _fcm_token: &str,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        if self.fail_on_update.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(())
    }

    async fn update_last_login(
        &self,
        _device_id: Uuid,
        _tenant_id: Uuid,
        _employee_id: Uuid,
        _employee_name: &str,
        _employee_role: &[String],
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        if self.fail_on_update.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(())
    }

    async fn list_fcm_devices(&self) -> Result<Vec<FcmDeviceRow>, sqlx::Error> {
        check_fail!(self);
        if self.return_data.load(Ordering::SeqCst) {
            if self.return_call_disabled.load(Ordering::SeqCst) {
                Ok(vec![FcmDeviceRow {
                    id: Uuid::nil(),
                    fcm_token: "mock-fcm-token-1".to_string(),
                    call_enabled: false,
                    call_schedule: None,
                }])
            } else if self.return_schedule_disabled.load(Ordering::SeqCst) {
                Ok(vec![FcmDeviceRow {
                    id: Uuid::nil(),
                    fcm_token: "mock-fcm-token-1".to_string(),
                    call_enabled: true,
                    call_schedule: Some(serde_json::json!({"enabled": false})),
                }])
            } else if self.return_schedule_with_days.load(Ordering::SeqCst) {
                // 全曜日 0-6 を含むスケジュール + 0:00-23:59 → 常に通知
                Ok(vec![FcmDeviceRow {
                    id: Uuid::nil(),
                    fcm_token: "mock-fcm-token-1".to_string(),
                    call_enabled: true,
                    call_schedule: Some(serde_json::json!({
                        "enabled": true,
                        "days": [0, 1, 2, 3, 4, 5, 6],
                        "startHour": 0,
                        "startMin": 0,
                        "endHour": 24,
                        "endMin": 0
                    })),
                }])
            } else if self.return_schedule_days_mismatch.load(Ordering::SeqCst) {
                // days に 99 のみ → 今日の曜日(0-6)は含まれない → skipped
                Ok(vec![FcmDeviceRow {
                    id: Uuid::nil(),
                    fcm_token: "mock-fcm-token-1".to_string(),
                    call_enabled: true,
                    call_schedule: Some(serde_json::json!({
                        "enabled": true,
                        "days": [99]
                    })),
                }])
            } else if self.return_schedule_overnight.load(Ordering::SeqCst) {
                // 深夜跨ぎスケジュール: 0:00-23:59 (start > end にならないよう 22:00-06:00)
                // ただし current >= start || current < end で常に true になるよう 0:01-0:00
                Ok(vec![FcmDeviceRow {
                    id: Uuid::nil(),
                    fcm_token: "mock-fcm-token-1".to_string(),
                    call_enabled: true,
                    call_schedule: Some(serde_json::json!({
                        "enabled": true,
                        "days": [0, 1, 2, 3, 4, 5, 6],
                        "startHour": 23,
                        "startMin": 59,
                        "endHour": 23,
                        "endMin": 58
                    })),
                }])
            } else {
                Ok(vec![FcmDeviceRow {
                    id: Uuid::nil(),
                    fcm_token: "mock-fcm-token-1".to_string(),
                    call_enabled: true,
                    call_schedule: None,
                }])
            }
        } else {
            Ok(vec![])
        }
    }

    async fn get_device_tenant_active(
        &self,
        _device_id: Uuid,
    ) -> Result<Option<DeviceTenantRow>, sqlx::Error> {
        check_fail!(self);
        if self.return_data.load(Ordering::SeqCst) {
            Ok(Some(DeviceTenantRow {
                tenant_id: Uuid::nil(),
            }))
        } else {
            Ok(None)
        }
    }

    async fn list_tenant_fcm_tokens_except(
        &self,
        _tenant_id: Uuid,
        _exclude_device_id: Uuid,
    ) -> Result<Vec<String>, sqlx::Error> {
        check_fail!(self);
        if self.return_fcm_tokens.load(Ordering::SeqCst) {
            Ok(vec!["mock-fcm-token-other-device".to_string()])
        } else {
            Ok(vec![])
        }
    }

    async fn list_all_callable_devices(&self) -> Result<Vec<FcmTestDeviceRow>, sqlx::Error> {
        check_fail!(self);
        if self.return_callable_devices.load(Ordering::SeqCst) {
            Ok(vec![FcmTestDeviceRow {
                id: Uuid::nil(),
                device_name: "Callable Device".to_string(),
                fcm_token: "mock-callable-token".to_string(),
            }])
        } else {
            Ok(vec![])
        }
    }

    async fn update_watchdog_state(
        &self,
        _device_id: Uuid,
        _tenant_id: Uuid,
        _running: bool,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        if self.fail_on_update.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(())
    }

    async fn report_version(
        &self,
        _device_id: Uuid,
        _tenant_id: Uuid,
        _version_code: i32,
        _version_name: &str,
        _is_device_owner: bool,
        _is_dev_device: bool,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        if self.fail_on_update.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(())
    }

    async fn list_dev_device_tenant_ids(&self) -> Result<Vec<String>, sqlx::Error> {
        check_fail!(self);
        if self.return_dev_tenants.load(Ordering::SeqCst) {
            Ok(vec![Uuid::nil().to_string()])
        } else {
            Ok(vec![])
        }
    }

    // --- Tenant-scoped ---

    async fn list_devices(&self, _tenant_id: Uuid) -> Result<Vec<DeviceRow>, sqlx::Error> {
        check_fail!(self);
        if self.return_device_rows.load(Ordering::SeqCst) {
            Ok(vec![DeviceRow {
                id: Uuid::nil(),
                tenant_id: _tenant_id,
                device_name: "Mock Device".to_string(),
                device_type: "android".to_string(),
                phone_number: Some("090-0000-0000".to_string()),
                user_id: None,
                status: "active".to_string(),
                approved_by: None,
                approved_at: None,
                last_seen_at: None,
                call_enabled: true,
                call_schedule: None,
                fcm_token: None,
                last_login_employee_id: None,
                last_login_employee_name: None,
                last_login_employee_role: None,
                app_version_code: None,
                app_version_name: None,
                is_device_owner: false,
                is_dev_device: false,
                always_on: false,
                watchdog_running: None,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
            }])
        } else {
            Ok(vec![])
        }
    }

    async fn list_pending(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<RegistrationRequestRow>, sqlx::Error> {
        check_fail!(self);
        if self.return_pending_rows.load(Ordering::SeqCst) {
            Ok(vec![RegistrationRequestRow {
                id: Uuid::nil(),
                registration_code: "123456".to_string(),
                flow_type: "qr_permanent".to_string(),
                tenant_id: Some(_tenant_id),
                phone_number: Some("090-0000-0000".to_string()),
                device_name: "Pending Device".to_string(),
                status: "pending".to_string(),
                device_id: None,
                expires_at: None,
                is_device_owner: false,
                is_dev_device: false,
                created_at: "2026-01-01T00:00:00Z".to_string(),
            }])
        } else {
            Ok(vec![])
        }
    }

    async fn create_url_token(
        &self,
        _tenant_id: Uuid,
        _code: &str,
        _device_name: &str,
        _is_device_owner: bool,
        _is_dev_device: bool,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn create_device_owner_token(
        &self,
        _tenant_id: Uuid,
        _code: &str,
        _device_name: &str,
        _is_dev_device: bool,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn create_permanent_qr(
        &self,
        _tenant_id: Uuid,
        _code: &str,
        _device_name: &str,
        _is_device_owner: bool,
        _is_dev_device: bool,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn find_approve_request(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<ApproveLookupRow>, sqlx::Error> {
        check_fail!(self);
        if self.return_data.load(Ordering::SeqCst) {
            Ok(Some(ApproveLookupRow {
                id: _id,
                flow_type: "qr_permanent".to_string(),
                phone_number: Some("090-1234-5678".to_string()),
                device_name: Some("Test Device".to_string()),
                status: "pending".to_string(),
                is_device_owner: false,
                is_dev_device: false,
            }))
        } else {
            Ok(None)
        }
    }

    async fn approve_device(
        &self,
        _tenant_id: Uuid,
        _req_id: Uuid,
        _device_name: &str,
        _device_type: &str,
        _phone_number: Option<&str>,
        _approved_by: Option<Uuid>,
        _is_device_owner: bool,
        _is_dev_device: bool,
    ) -> Result<Uuid, sqlx::Error> {
        check_fail!(self);
        if self.fail_on_approve.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(Uuid::nil())
    }

    async fn find_approve_by_code_request(
        &self,
        _tenant_id: Uuid,
        _code: &str,
    ) -> Result<Option<ApproveLookupRow>, sqlx::Error> {
        check_fail!(self);
        if self.return_data.load(Ordering::SeqCst) {
            Ok(Some(ApproveLookupRow {
                id: Uuid::nil(),
                flow_type: "qr_temporary".to_string(),
                phone_number: Some("090-0000-1111".to_string()),
                device_name: Some("QR Device".to_string()),
                status: "pending".to_string(),
                is_device_owner: false,
                is_dev_device: false,
            }))
        } else {
            Ok(None)
        }
    }

    async fn approve_by_code(
        &self,
        _tenant_id: Uuid,
        _req_id: Uuid,
        _device_name: &str,
        _device_type: &str,
        _phone_number: Option<&str>,
        _approved_by: Option<Uuid>,
        _is_device_owner: bool,
        _is_dev_device: bool,
    ) -> Result<Uuid, sqlx::Error> {
        check_fail!(self);
        if self.fail_on_approve.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(Uuid::nil())
    }

    async fn reject_device(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_data.load(Ordering::SeqCst))
    }

    async fn disable_device(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_data.load(Ordering::SeqCst))
    }

    async fn enable_device(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_data.load(Ordering::SeqCst))
    }

    async fn delete_device(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_data.load(Ordering::SeqCst))
    }

    async fn update_call_settings(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _call_enabled: bool,
        _call_schedule: Option<&serde_json::Value>,
        _always_on: Option<bool>,
    ) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_data.load(Ordering::SeqCst))
    }

    async fn get_fcm_token_bypass_rls(
        &self,
        _device_id: Uuid,
    ) -> Result<Option<Option<String>>, sqlx::Error> {
        check_fail!(self);
        if self.return_data.load(Ordering::SeqCst) {
            if self.return_no_fcm_token.load(Ordering::SeqCst) {
                Ok(Some(None))
            } else {
                Ok(Some(Some("mock-fcm-token-bypass".to_string())))
            }
        } else {
            Ok(None)
        }
    }

    async fn get_device_fcm_token(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<Option<String>>, sqlx::Error> {
        check_fail!(self);
        if self.return_data.load(Ordering::SeqCst) {
            if self.return_null_fcm_token.load(Ordering::SeqCst) {
                Ok(Some(None))
            } else {
                Ok(Some(Some("mock-fcm-token".to_string())))
            }
        } else {
            Ok(None)
        }
    }

    async fn list_tenant_fcm_devices(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<FcmTestDeviceRow>, sqlx::Error> {
        check_fail!(self);
        if self.return_data.load(Ordering::SeqCst) {
            Ok(vec![FcmTestDeviceRow {
                id: Uuid::nil(),
                device_name: "Test Device".to_string(),
                fcm_token: "mock-fcm-token-tenant".to_string(),
            }])
        } else {
            Ok(vec![])
        }
    }

    async fn list_ota_devices(
        &self,
        _tenant_id: Uuid,
        _dev_only: bool,
    ) -> Result<Vec<OtaDeviceRow>, sqlx::Error> {
        check_fail!(self);
        if self.return_data.load(Ordering::SeqCst) {
            if self.return_ota_no_version.load(Ordering::SeqCst) {
                Ok(vec![OtaDeviceRow {
                    id: Uuid::nil(),
                    device_name: "OTA Device".to_string(),
                    fcm_token: "mock-fcm-token-ota".to_string(),
                    app_version_code: None,
                }])
            } else {
                Ok(vec![OtaDeviceRow {
                    id: Uuid::nil(),
                    device_name: "OTA Device".to_string(),
                    fcm_token: "mock-fcm-token-ota".to_string(),
                    app_version_code: Some(10),
                }])
            }
        } else {
            Ok(vec![])
        }
    }
}

// ============================================================
// MockDriverInfoRepository
// ============================================================

pub struct MockDriverInfoRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockDriverInfoRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl DriverInfoRepository for MockDriverInfoRepository {
    async fn get_employee(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
    ) -> Result<Option<Employee>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn get_health_baseline(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
    ) -> Result<Option<EmployeeHealthBaseline>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn get_recent_measurements(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
    ) -> Result<Vec<MeasurementSummary>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn get_working_hours(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
    ) -> Result<Vec<DtakoDailyWorkHours>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn get_past_instructions(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
    ) -> Result<Vec<InstructionSummary>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn get_carrying_items(&self, _tenant_id: Uuid) -> Result<Vec<CarryingItem>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn get_past_tenko_records(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
    ) -> Result<Vec<TenkoRecord>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn get_recent_daily_inspections(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
    ) -> Result<Vec<DailyInspectionSummary>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn get_equipment_failures(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<EquipmentFailure>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }
}

// ============================================================
// MockDtakoCsvProxyRepository
// ============================================================

pub struct MockDtakoCsvProxyRepository {
    pub fail_next: AtomicBool,
    pub return_prefix: std::sync::Mutex<Option<String>>,
}

impl Default for MockDtakoCsvProxyRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            return_prefix: std::sync::Mutex::new(None),
        }
    }
}

#[async_trait::async_trait]
impl DtakoCsvProxyRepository for MockDtakoCsvProxyRepository {
    async fn get_r2_key_prefix(
        &self,
        _tenant_id: Uuid,
        _unko_no: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_prefix.lock().unwrap().clone())
    }
}

// ============================================================
// MockDtakoDailyHoursRepository
// ============================================================

pub struct MockDtakoDailyHoursRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockDtakoDailyHoursRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl DtakoDailyHoursRepository for MockDtakoDailyHoursRepository {
    async fn count(
        &self,
        _tenant_id: Uuid,
        _driver_id: Option<Uuid>,
        _date_from: Option<NaiveDate>,
        _date_to: Option<NaiveDate>,
    ) -> Result<i64, sqlx::Error> {
        check_fail!(self);
        Ok(0)
    }

    async fn list(
        &self,
        _tenant_id: Uuid,
        _driver_id: Option<Uuid>,
        _date_from: Option<NaiveDate>,
        _date_to: Option<NaiveDate>,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<DtakoDailyWorkHours>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn get_segments(
        &self,
        _tenant_id: Uuid,
        _driver_id: Uuid,
        _date: NaiveDate,
    ) -> Result<Vec<DtakoDailyWorkSegment>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }
}

// ============================================================
// MockDtakoDriversRepository
// ============================================================

pub struct MockDtakoDriversRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockDtakoDriversRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl DtakoDriversRepository for MockDtakoDriversRepository {
    async fn list(&self, _tenant_id: Uuid) -> Result<Vec<Driver>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }
}

// ============================================================
// MockDtakoLogsRepository
// ============================================================

pub struct MockDtakoLogsRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockDtakoLogsRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl DtakoLogsRepository for MockDtakoLogsRepository {
    async fn bulk_upsert(
        &self,
        _tenant_id: Uuid,
        records: &[alc_core::models::DtakologInput],
    ) -> Result<u64, sqlx::Error> {
        check_fail!(self);
        Ok(records.len() as u64)
    }

    async fn current_list_all(&self, _tenant_id: Uuid) -> Result<Vec<DtakologRow>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn get_date(
        &self,
        _tenant_id: Uuid,
        _date_time: &str,
        _vehicle_cd: Option<i32>,
    ) -> Result<Vec<DtakologRow>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn current_list_select(
        &self,
        _tenant_id: Uuid,
        _address_disp_p: Option<&str>,
        _branch_cd: Option<i32>,
        _vehicle_cds: &[i32],
    ) -> Result<Vec<DtakologRow>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn get_date_range(
        &self,
        _tenant_id: Uuid,
        _start: &str,
        _end: &str,
        _vehicle_cd: Option<i32>,
    ) -> Result<Vec<DtakologRow>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }
}

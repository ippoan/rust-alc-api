use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

use rust_alc_api::db::models::*;
use rust_alc_api::db::repository::auth::{AuthRepository, SsoConfigRow};
use rust_alc_api::db::repository::bot_admin::{BotAdminRepository, BotConfigRow};
use rust_alc_api::db::repository::car_inspections::{
    CarInspectionFile, CarInspectionRepository, VehicleCategories,
};
use rust_alc_api::db::repository::carins_files::CarinsFilesRepository;
use rust_alc_api::db::repository::carrying_items::CarryingItemsRepository;
use rust_alc_api::db::repository::communication_items::{
    CommunicationItemWithName, CommunicationItemsRepository,
};
use rust_alc_api::db::repository::daily_health::DailyHealthRepository;
use rust_alc_api::db::repository::devices::{
    ApproveLookupRow, ClaimLookupRow, CreateRegistrationResult, DeviceRepository, DeviceRow,
    DeviceSettingsRow, DeviceTenantRow, FcmDeviceRow, FcmTestDeviceRow, OtaDeviceRow,
    RegistrationRequestRow, RegistrationStatusRow,
};
use rust_alc_api::db::repository::driver_info::DriverInfoRepository;
use rust_alc_api::db::repository::dtako_csv_proxy::DtakoCsvProxyRepository;
use rust_alc_api::db::repository::dtako_daily_hours::DtakoDailyHoursRepository;
use rust_alc_api::db::repository::dtako_drivers::{Driver, DtakoDriversRepository};
use rust_alc_api::routes::carins_files::FileRow;
use rust_alc_api::routes::daily_health::DailyHealthRow;
use rust_alc_api::routes::driver_info::{
    DailyInspectionSummary, InstructionSummary, MeasurementSummary,
};

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

pub struct MockAuthRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockAuthRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
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
        Ok(None)
    }

    async fn find_user_by_lineworks_id(
        &self,
        _lineworks_id: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn find_user_by_refresh_token_hash(
        &self,
        _token_hash: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn find_invitation_by_email(
        &self,
        _email: &str,
    ) -> Result<Option<TenantAllowedEmail>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
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
        Ok(None)
    }

    async fn create_tenant_with_domain(&self, _email_domain: &str) -> Result<Tenant, sqlx::Error> {
        check_fail!(self);
        todo!("MockAuthRepository::create_tenant_with_domain")
    }

    async fn create_tenant_by_name(&self, _name: &str) -> Result<Tenant, sqlx::Error> {
        check_fail!(self);
        todo!("MockAuthRepository::create_tenant_by_name")
    }

    async fn get_tenant_by_id(&self, _id: Uuid) -> Result<Option<Tenant>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn get_tenant_slug(&self, _tenant_id: Uuid) -> Result<Option<String>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn create_user_google(
        &self,
        _tenant_id: Uuid,
        _google_sub: &str,
        _email: &str,
        _name: &str,
        _role: &str,
    ) -> Result<User, sqlx::Error> {
        check_fail!(self);
        todo!("MockAuthRepository::create_user_google")
    }

    async fn create_user_lineworks(
        &self,
        _tenant_id: Uuid,
        _lineworks_id: &str,
        _email: &str,
        _name: &str,
    ) -> Result<User, sqlx::Error> {
        check_fail!(self);
        todo!("MockAuthRepository::create_user_lineworks")
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
        Ok(None)
    }

    async fn resolve_sso_config_required(
        &self,
        _provider: &str,
        _domain: &str,
    ) -> Result<SsoConfigRow, sqlx::Error> {
        check_fail!(self);
        todo!("MockAuthRepository::resolve_sso_config_required")
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
}

// ============================================================
// MockCarinsFilesRepository
// ============================================================

pub struct MockCarinsFilesRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockCarinsFilesRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
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
        Ok(None)
    }

    async fn get_file_for_download(
        &self,
        _tenant_id: Uuid,
        _uuid: &str,
    ) -> Result<Option<FileRow>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
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
        todo!("MockCarinsFilesRepository::create_file")
    }

    async fn delete_file(&self, _tenant_id: Uuid, _uuid: &str) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(false)
    }

    async fn restore_file(&self, _tenant_id: Uuid, _uuid: &str) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(false)
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
}

impl Default for MockDeviceRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl DeviceRepository for MockDeviceRepository {
    // --- Public (no tenant context) ---

    async fn code_exists(&self, _code: &str) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(false)
    }

    async fn create_registration_request(
        &self,
        _code: &str,
        _device_name: &str,
    ) -> Result<CreateRegistrationResult, sqlx::Error> {
        check_fail!(self);
        todo!("MockDeviceRepository::create_registration_request")
    }

    async fn get_registration_status(
        &self,
        _code: &str,
    ) -> Result<Option<RegistrationStatusRow>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn is_expired(&self, _expires_at: &str) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(false)
    }

    async fn find_claim_request(&self, _code: &str) -> Result<Option<ClaimLookupRow>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
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
        Ok(None)
    }

    async fn lookup_device_tenant(&self, _device_id: Uuid) -> Result<Option<Uuid>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn update_fcm_token(
        &self,
        _device_id: Uuid,
        _tenant_id: Uuid,
        _fcm_token: &str,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
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
        Ok(())
    }

    async fn list_fcm_devices(&self) -> Result<Vec<FcmDeviceRow>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn get_device_tenant_active(
        &self,
        _device_id: Uuid,
    ) -> Result<Option<DeviceTenantRow>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn list_tenant_fcm_tokens_except(
        &self,
        _tenant_id: Uuid,
        _exclude_device_id: Uuid,
    ) -> Result<Vec<String>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn list_all_callable_devices(&self) -> Result<Vec<FcmTestDeviceRow>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn update_watchdog_state(
        &self,
        _device_id: Uuid,
        _tenant_id: Uuid,
        _running: bool,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
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
        Ok(())
    }

    async fn list_dev_device_tenant_ids(&self) -> Result<Vec<String>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    // --- Tenant-scoped ---

    async fn list_devices(&self, _tenant_id: Uuid) -> Result<Vec<DeviceRow>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn list_pending(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<RegistrationRequestRow>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
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
        Ok(None)
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
        Ok(Uuid::nil())
    }

    async fn find_approve_by_code_request(
        &self,
        _tenant_id: Uuid,
        _code: &str,
    ) -> Result<Option<ApproveLookupRow>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
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
        Ok(Uuid::nil())
    }

    async fn reject_device(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(false)
    }

    async fn disable_device(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(false)
    }

    async fn enable_device(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(false)
    }

    async fn delete_device(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(false)
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
        Ok(false)
    }

    async fn get_fcm_token_bypass_rls(
        &self,
        _device_id: Uuid,
    ) -> Result<Option<Option<String>>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn get_device_fcm_token(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<Option<String>>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn list_tenant_fcm_devices(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<FcmTestDeviceRow>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn list_ota_devices(
        &self,
        _tenant_id: Uuid,
        _dev_only: bool,
    ) -> Result<Vec<OtaDeviceRow>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
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

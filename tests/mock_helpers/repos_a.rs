use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

use rust_alc_api::db::models::DtakologRow;
use rust_alc_api::db::models::*;
use rust_alc_api::db::repository::auth::{AuthRepository, SsoConfigRow};
use rust_alc_api::db::repository::bot_admin::{
    BotAdminRepository, BotConfigExportRow, TenantInfoForExport,
};
use rust_alc_api::db::repository::car_inspections::{
    CarInspectionFile, CarInspectionRepository, VehicleCategories,
};
use rust_alc_api::db::repository::carins_files::CarinsFilesRepository;
use rust_alc_api::db::repository::carins_files::FileRow;
use rust_alc_api::db::repository::daily_health::DailyHealthRepository;
use rust_alc_api::db::repository::daily_health::DailyHealthRow;
use rust_alc_api::db::repository::driver_info::DriverInfoRepository;
use rust_alc_api::db::repository::driver_info::{
    DailyInspectionSummary, InstructionSummary, MaintenanceRecordSummary, MeasurementSummary,
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

/// Helper to create a dummy User for tests
pub fn mock_user(tenant_id: Uuid) -> User {
    User {
        id: Uuid::new_v4(),
        tenant_id,
        google_sub: Some("test-google-sub-12345".to_string()),
        lineworks_id: None,
        line_user_id: None,
        email: "google-test@example.com".to_string(),
        name: "Google Test User".to_string(),
        role: "admin".to_string(),
        username: None,
        password_hash: None,
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
    /// If Some, find_user_by_line_user_id returns this user
    pub return_line_user: std::sync::Mutex<Option<User>>,
    /// find_recipients_by_line_user_id returns Vec<(tenant_id, name)>
    pub return_line_recipients: std::sync::Mutex<Vec<(Uuid, String)>>,
    /// If Some, find_user_in_tenant returns this user
    pub return_switch_user: std::sync::Mutex<Option<User>>,
    /// If Some, find_user_by_username returns this user
    pub return_username_user: std::sync::Mutex<Option<User>>,
    /// If Some, get_tenant_slug returns this slug
    pub return_slug: std::sync::Mutex<Option<String>>,
    /// If Some, get_tenant_short_id returns this short_id
    pub return_short_id: std::sync::Mutex<Option<String>>,
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
            return_line_user: std::sync::Mutex::new(None),
            return_line_recipients: std::sync::Mutex::new(Vec::new()),
            return_switch_user: std::sync::Mutex::new(None),
            return_username_user: std::sync::Mutex::new(None),
            return_slug: std::sync::Mutex::new(None),
            // mock のデフォルトを Some にしておくと、google_login 等で
            // get_tenant_short_id (NOT NULL 期待) を踏む経路が落ちずに済む。
            // 個別テストで上書きしたければ return_short_id を直接いじる。
            return_short_id: std::sync::Mutex::new(Some("mockshrt".to_string())),
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

    async fn delete_invitation(&self, _tenant_id: Uuid, _id: Uuid) -> Result<(), sqlx::Error> {
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
            short_id: format!("{:.8}", tid.simple()),
            created_at: Utc::now(),
        })
    }

    async fn ensure_tenant_exists(&self, _tenant_id: Uuid) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn get_tenant_by_id(&self, _id: Uuid) -> Result<Option<Tenant>, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_tenant.lock().unwrap().clone())
    }

    async fn get_tenant_slug(&self, _tenant_id: Uuid) -> Result<Option<String>, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_slug.lock().unwrap().clone())
    }

    async fn get_tenant_short_id(&self, _tenant_id: Uuid) -> Result<Option<String>, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_short_id.lock().unwrap().clone())
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
            line_user_id: None,
            email: email.to_string(),
            name: name.to_string(),
            role: role.to_string(),
            username: None,
            password_hash: None,
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
            line_user_id: None,
            email: email.to_string(),
            name: name.to_string(),
            role: "admin".to_string(),
            username: None,
            password_hash: None,
            refresh_token_hash: None,
            refresh_token_expires_at: None,
            created_at: Utc::now(),
        })
    }

    // --- Switch org ---

    async fn find_user_in_tenant(
        &self,
        _target_tenant_id: Uuid,
        _current_user_id: Uuid,
    ) -> Result<Option<User>, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_switch_user.lock().unwrap().clone())
    }

    // --- Password login ---

    async fn find_user_by_username(
        &self,
        _tenant_id: Uuid,
        _username: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_username_user.lock().unwrap().clone())
    }

    // --- LINE Login ---

    async fn find_user_by_line_user_id(
        &self,
        _line_user_id: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_line_user.lock().unwrap().clone())
    }

    async fn find_recipients_by_line_user_id(
        &self,
        _line_user_id: &str,
    ) -> Result<Vec<(Uuid, String)>, sqlx::Error> {
        check_fail!(self);
        Ok(self.return_line_recipients.lock().unwrap().clone())
    }

    async fn register_line_recipient(
        &self,
        _tenant_id: Uuid,
        _name: &str,
        _line_user_id: &str,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn create_user_line(
        &self,
        tenant_id: Uuid,
        line_user_id: &str,
        name: &str,
    ) -> Result<User, sqlx::Error> {
        check_fail!(self);
        Ok(User {
            id: Uuid::new_v4(),
            tenant_id,
            google_sub: None,
            lineworks_id: None,
            line_user_id: Some(line_user_id.to_string()),
            email: line_user_id.to_string(),
            name: name.to_string(),
            role: "viewer".to_string(),
            username: None,
            password_hash: None,
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

    async fn clear_refresh_token(
        &self,
        _tenant_id: Uuid,
        _user_id: Uuid,
    ) -> Result<(), sqlx::Error> {
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
    pub return_tenant_for_export: std::sync::Mutex<Option<TenantInfoForExport>>,
    pub return_configs_for_export: std::sync::Mutex<Vec<BotConfigExportRow>>,
    pub fail_tenant_for_export: AtomicBool,
    pub fail_configs_for_export: AtomicBool,
}

impl Default for MockBotAdminRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            return_tenant_for_export: std::sync::Mutex::new(None),
            return_configs_for_export: std::sync::Mutex::new(Vec::new()),
            fail_tenant_for_export: AtomicBool::new(false),
            fail_configs_for_export: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl BotAdminRepository for MockBotAdminRepository {
    async fn get_tenant_for_export(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Option<TenantInfoForExport>, sqlx::Error> {
        if self.fail_tenant_for_export.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(self.return_tenant_for_export.lock().unwrap().clone())
    }

    async fn list_configs_for_export(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<BotConfigExportRow>, sqlx::Error> {
        if self.fail_configs_for_export.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(self.return_configs_for_export.lock().unwrap().clone())
    }
}

// ============================================================
// MockCarInspectionRepository
// ============================================================

pub struct MockCarInspectionRepository {
    pub fail_next: AtomicBool,
    pub pending_pdf_uuid: std::sync::Mutex<Option<String>>,
}

impl Default for MockCarInspectionRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            pending_pdf_uuid: std::sync::Mutex::new(None),
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

    async fn list_by_car_id(
        &self,
        _tenant_id: Uuid,
        _car_id: &str,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
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
        Ok(self.pending_pdf_uuid.lock().unwrap().clone())
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

    async fn lookup_expiry(
        &self,
        _tenant_id: Uuid,
        _cert_no: Option<&str>,
        _car_id: Option<&str>,
    ) -> Result<alc_core::repository::car_inspections::CarinsLookup, sqlx::Error> {
        check_fail!(self);
        Ok(alc_core::repository::car_inspections::CarinsLookup {
            expires_on: None,
            matched_by: "none",
            car_no: None,
        })
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
            storage_verified: Some(true),
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

    async fn update_storage_verified(
        &self,
        _tenant_id: Uuid,
        _uuid: &str,
        _verified: bool,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
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
// MockDriverInfoRepository
// ============================================================

pub struct MockDriverInfoRepository {
    pub fail_next: AtomicBool,
    /// get_employee が返す Employee (Refs ippoan/alc-app#315: 判定した運行管理者の
    /// ロール検証用)。None (既定) なら get_employee は Ok(None) を返す
    pub employee: std::sync::Mutex<Option<Employee>>,
}

impl Default for MockDriverInfoRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            employee: std::sync::Mutex::new(None),
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
        Ok(self.employee.lock().unwrap().clone())
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

    async fn get_recent_maintenance_records(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
    ) -> Result<Vec<MaintenanceRecordSummary>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }
}

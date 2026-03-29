use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, Utc};
use uuid::Uuid;

use rust_alc_api::db::models::*;
use rust_alc_api::db::repository::nfc_tags::NfcTagRepository;
use rust_alc_api::db::repository::sso_admin::{SsoAdminRepository, SsoConfigRow};
use rust_alc_api::db::repository::tenant_users::{TenantUsersRepository, UserRow};
use rust_alc_api::db::repository::tenko_call::{
    DriverInfo, RegisterDriverResult, TenkoCallDriverRow, TenkoCallNumberRow, TenkoCallRepository,
};
use rust_alc_api::db::repository::tenko_records::TenkoRecordsRepository;
use rust_alc_api::db::repository::tenko_schedules::{ScheduleListResult, TenkoSchedulesRepository};
use rust_alc_api::db::repository::tenko_sessions::{SessionListResult, TenkoSessionRepository};
use rust_alc_api::db::repository::tenko_webhooks::TenkoWebhooksRepository;
use rust_alc_api::db::repository::timecard::{TimePunchCsvRow, TimecardRepository};

macro_rules! check_fail {
    ($self:expr) => {
        if $self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
    };
}

// ---------------------------------------------------------------------------
// MockNfcTagRepository
// ---------------------------------------------------------------------------

pub struct MockNfcTagRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockNfcTagRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl NfcTagRepository for MockNfcTagRepository {
    async fn search_by_uuid(
        &self,
        _tenant_id: Uuid,
        _nfc_uuid: &str,
    ) -> Result<Option<NfcTag>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn get_car_inspection_json(
        &self,
        _tenant_id: Uuid,
        _car_inspection_id: i32,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn list(
        &self,
        _tenant_id: Uuid,
        _car_inspection_id: Option<i32>,
    ) -> Result<Vec<NfcTag>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn register(
        &self,
        _tenant_id: Uuid,
        _nfc_uuid: &str,
        _car_inspection_id: i32,
    ) -> Result<NfcTag, sqlx::Error> {
        check_fail!(self);
        todo!("MockNfcTagRepository::register")
    }

    async fn delete(&self, _tenant_id: Uuid, _nfc_uuid: &str) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(false)
    }
}

// ---------------------------------------------------------------------------
// MockSsoAdminRepository
// ---------------------------------------------------------------------------

pub struct MockSsoAdminRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockSsoAdminRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl SsoAdminRepository for MockSsoAdminRepository {
    async fn list_configs(&self, _tenant_id: Uuid) -> Result<Vec<SsoConfigRow>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn upsert_config_with_secret(
        &self,
        _tenant_id: Uuid,
        _provider: &str,
        _client_id: &str,
        _client_secret_encrypted: &str,
        _external_org_id: &str,
        _woff_id: Option<&str>,
        _enabled: bool,
    ) -> Result<SsoConfigRow, sqlx::Error> {
        check_fail!(self);
        Ok(SsoConfigRow {
            provider: _provider.to_string(),
            client_id: _client_id.to_string(),
            external_org_id: _external_org_id.to_string(),
            enabled: _enabled,
            woff_id: _woff_id.map(|s| s.to_string()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
    }

    async fn upsert_config_without_secret(
        &self,
        _tenant_id: Uuid,
        _provider: &str,
        _client_id: &str,
        _external_org_id: &str,
        _woff_id: Option<&str>,
        _enabled: bool,
    ) -> Result<SsoConfigRow, sqlx::Error> {
        check_fail!(self);
        Ok(SsoConfigRow {
            provider: _provider.to_string(),
            client_id: _client_id.to_string(),
            external_org_id: _external_org_id.to_string(),
            enabled: _enabled,
            woff_id: _woff_id.map(|s| s.to_string()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
    }

    async fn delete_config(&self, _tenant_id: Uuid, _provider: &str) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// MockTenantUsersRepository
// ---------------------------------------------------------------------------

pub struct MockTenantUsersRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockTenantUsersRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl TenantUsersRepository for MockTenantUsersRepository {
    async fn list_users(&self, _tenant_id: Uuid) -> Result<Vec<UserRow>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn list_invitations(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<TenantAllowedEmail>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn invite_user(
        &self,
        _tenant_id: Uuid,
        _email: &str,
        _role: &str,
    ) -> Result<TenantAllowedEmail, sqlx::Error> {
        check_fail!(self);
        todo!("MockTenantUsersRepository::invite_user")
    }

    async fn delete_invitation(&self, _tenant_id: Uuid, _id: Uuid) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn delete_user(&self, _tenant_id: Uuid, _id: Uuid) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// MockTenkoCallRepository
// ---------------------------------------------------------------------------

pub struct MockTenkoCallRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockTenkoCallRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl TenkoCallRepository for MockTenkoCallRepository {
    async fn register_driver(
        &self,
        _call_number: &str,
        _phone_number: &str,
        _driver_name: &str,
        _employee_code: Option<&str>,
    ) -> Result<Option<RegisterDriverResult>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn record_tenko(
        &self,
        _phone_number: &str,
        _driver_name: &str,
        _latitude: f64,
        _longitude: f64,
    ) -> Result<Option<DriverInfo>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn list_numbers(&self) -> Result<Vec<TenkoCallNumberRow>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn create_number(
        &self,
        _call_number: &str,
        _tenant_id: &str,
        _label: Option<&str>,
    ) -> Result<i32, sqlx::Error> {
        check_fail!(self);
        Ok(0)
    }

    async fn delete_number(&self, _id: i32) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn list_drivers(&self) -> Result<Vec<TenkoCallDriverRow>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }
}

// ---------------------------------------------------------------------------
// MockTenkoRecordsRepository
// ---------------------------------------------------------------------------

pub struct MockTenkoRecordsRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockTenkoRecordsRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl TenkoRecordsRepository for MockTenkoRecordsRepository {
    async fn count(
        &self,
        _tenant_id: Uuid,
        _filter: &TenkoRecordFilter,
    ) -> Result<i64, sqlx::Error> {
        check_fail!(self);
        Ok(0)
    }

    async fn list(
        &self,
        _tenant_id: Uuid,
        _filter: &TenkoRecordFilter,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<TenkoRecord>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn get(&self, _tenant_id: Uuid, _id: Uuid) -> Result<Option<TenkoRecord>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn list_all(
        &self,
        _tenant_id: Uuid,
        _filter: &TenkoRecordFilter,
    ) -> Result<Vec<TenkoRecord>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }
}

// ---------------------------------------------------------------------------
// MockTenkoSchedulesRepository
// ---------------------------------------------------------------------------

pub struct MockTenkoSchedulesRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockTenkoSchedulesRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl TenkoSchedulesRepository for MockTenkoSchedulesRepository {
    async fn create(
        &self,
        _tenant_id: Uuid,
        _input: &CreateTenkoSchedule,
    ) -> Result<TenkoSchedule, sqlx::Error> {
        check_fail!(self);
        todo!("MockTenkoSchedulesRepository::create")
    }

    async fn batch_create(
        &self,
        _tenant_id: Uuid,
        _inputs: &[CreateTenkoSchedule],
    ) -> Result<Vec<TenkoSchedule>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn list(
        &self,
        _tenant_id: Uuid,
        _filter: &TenkoScheduleFilter,
        _page: i64,
        _per_page: i64,
    ) -> Result<ScheduleListResult, sqlx::Error> {
        check_fail!(self);
        Ok(ScheduleListResult {
            schedules: vec![],
            total: 0,
        })
    }

    async fn get(&self, _tenant_id: Uuid, _id: Uuid) -> Result<Option<TenkoSchedule>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn update(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _input: &UpdateTenkoSchedule,
    ) -> Result<Option<TenkoSchedule>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn delete(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(false)
    }

    async fn get_pending(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
    ) -> Result<Vec<TenkoSchedule>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }
}

// ---------------------------------------------------------------------------
// MockTenkoSessionRepository
// ---------------------------------------------------------------------------

pub struct MockTenkoSessionRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockTenkoSessionRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl TenkoSessionRepository for MockTenkoSessionRepository {
    async fn get(&self, _tenant_id: Uuid, _id: Uuid) -> Result<Option<TenkoSession>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn list(
        &self,
        _tenant_id: Uuid,
        _filter: &TenkoSessionFilter,
        _page: i64,
        _per_page: i64,
    ) -> Result<SessionListResult, sqlx::Error> {
        check_fail!(self);
        Ok(SessionListResult {
            sessions: vec![],
            total: 0,
        })
    }

    async fn get_schedule_unconsumed(
        &self,
        _tenant_id: Uuid,
        _schedule_id: Uuid,
    ) -> Result<Option<TenkoSchedule>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn consume_schedule(
        &self,
        _tenant_id: Uuid,
        _schedule_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn set_consumed_by_session(
        &self,
        _tenant_id: Uuid,
        _schedule_id: Uuid,
        _session_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn get_schedule_instruction(
        &self,
        _tenant_id: Uuid,
        _schedule_id: Option<Uuid>,
    ) -> Result<Option<String>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn create_session(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
        _schedule_id: Option<Uuid>,
        _tenko_type: &str,
        _initial_status: &str,
        _identity_face_photo_url: &Option<String>,
        _location: &Option<String>,
        _responsible_manager_name: &Option<String>,
    ) -> Result<TenkoSession, sqlx::Error> {
        check_fail!(self);
        todo!("MockTenkoSessionRepository::create_session")
    }

    async fn update_alcohol(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _next_status: &str,
        _measurement_id: Option<Uuid>,
        _alcohol_result: &str,
        _alcohol_value: f64,
        _alcohol_face_photo_url: &Option<String>,
        _cancel_reason: &Option<String>,
        _completed_at: Option<DateTime<Utc>>,
    ) -> Result<TenkoSession, sqlx::Error> {
        check_fail!(self);
        todo!("MockTenkoSessionRepository::update_alcohol")
    }

    async fn update_medical(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _temperature: Option<f64>,
        _systolic: Option<i32>,
        _diastolic: Option<i32>,
        _pulse: Option<i32>,
        _medical_measured_at: Option<DateTime<Utc>>,
        _medical_manual_input: Option<bool>,
    ) -> Result<TenkoSession, sqlx::Error> {
        check_fail!(self);
        todo!("MockTenkoSessionRepository::update_medical")
    }

    async fn confirm_instruction(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<TenkoSession, sqlx::Error> {
        check_fail!(self);
        todo!("MockTenkoSessionRepository::confirm_instruction")
    }

    async fn update_report(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _next_status: &str,
        _vehicle_road_status: &str,
        _driver_alternation: &str,
        _vehicle_road_audio_url: &Option<String>,
        _driver_alternation_audio_url: &Option<String>,
        _completed_at: Option<DateTime<Utc>>,
    ) -> Result<TenkoSession, sqlx::Error> {
        check_fail!(self);
        todo!("MockTenkoSessionRepository::update_report")
    }

    async fn cancel(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _reason: &Option<String>,
    ) -> Result<TenkoSession, sqlx::Error> {
        check_fail!(self);
        todo!("MockTenkoSessionRepository::cancel")
    }

    async fn update_self_declaration(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _declaration_json: &serde_json::Value,
    ) -> Result<TenkoSession, sqlx::Error> {
        check_fail!(self);
        todo!("MockTenkoSessionRepository::update_self_declaration")
    }

    async fn update_safety_judgment(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _next_status: &str,
        _judgment_json: &serde_json::Value,
        _interrupted_at: Option<DateTime<Utc>>,
    ) -> Result<TenkoSession, sqlx::Error> {
        check_fail!(self);
        todo!("MockTenkoSessionRepository::update_safety_judgment")
    }

    async fn update_daily_inspection(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _next_status: &str,
        _inspection_json: &serde_json::Value,
        _cancel_reason: &Option<String>,
        _completed_at: Option<DateTime<Utc>>,
    ) -> Result<TenkoSession, sqlx::Error> {
        check_fail!(self);
        todo!("MockTenkoSessionRepository::update_daily_inspection")
    }

    async fn update_carrying_items(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _carrying_json: &serde_json::Value,
    ) -> Result<TenkoSession, sqlx::Error> {
        check_fail!(self);
        todo!("MockTenkoSessionRepository::update_carrying_items")
    }

    async fn interrupt(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _reason: &Option<String>,
    ) -> Result<TenkoSession, sqlx::Error> {
        check_fail!(self);
        todo!("MockTenkoSessionRepository::interrupt")
    }

    async fn resume(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _resume_to: &str,
        _reason: &str,
        _resumed_by_user_id: Option<Uuid>,
    ) -> Result<TenkoSession, sqlx::Error> {
        check_fail!(self);
        todo!("MockTenkoSessionRepository::resume")
    }

    async fn get_carrying_item_name(
        &self,
        _tenant_id: Uuid,
        _item_id: Uuid,
    ) -> Result<Option<String>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn upsert_carrying_item_check(
        &self,
        _tenant_id: Uuid,
        _session_id: Uuid,
        _item_id: Uuid,
        _item_name: &str,
        _checked: bool,
        _checked_at: Option<DateTime<Utc>>,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn count_carrying_items(&self, _tenant_id: Uuid) -> Result<i64, sqlx::Error> {
        check_fail!(self);
        Ok(0)
    }

    async fn get_employee_name(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
    ) -> Result<Option<String>, sqlx::Error> {
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

    async fn create_tenko_record(
        &self,
        _tenant_id: Uuid,
        _session: &TenkoSession,
        _employee_name: &str,
        _instruction: &Option<String>,
        _record_data: &serde_json::Value,
        _record_hash: &str,
    ) -> Result<TenkoRecord, sqlx::Error> {
        check_fail!(self);
        todo!("MockTenkoSessionRepository::create_tenko_record")
    }

    async fn dashboard(
        &self,
        _tenant_id: Uuid,
        _overdue_minutes: i64,
    ) -> Result<TenkoDashboard, sqlx::Error> {
        check_fail!(self);
        Ok(TenkoDashboard {
            pending_schedules: 0,
            active_sessions: 0,
            interrupted_sessions: 0,
            completed_today: 0,
            cancelled_today: 0,
            overdue_schedules: vec![],
        })
    }
}

// ---------------------------------------------------------------------------
// MockTenkoWebhooksRepository
// ---------------------------------------------------------------------------

pub struct MockTenkoWebhooksRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockTenkoWebhooksRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl TenkoWebhooksRepository for MockTenkoWebhooksRepository {
    async fn upsert(
        &self,
        _tenant_id: Uuid,
        _input: &CreateWebhookConfig,
    ) -> Result<WebhookConfig, sqlx::Error> {
        check_fail!(self);
        todo!("MockTenkoWebhooksRepository::upsert")
    }

    async fn list(&self, _tenant_id: Uuid) -> Result<Vec<WebhookConfig>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn get(&self, _tenant_id: Uuid, _id: Uuid) -> Result<Option<WebhookConfig>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn delete(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(false)
    }

    async fn list_deliveries(
        &self,
        _tenant_id: Uuid,
        _config_id: Uuid,
    ) -> Result<Vec<WebhookDelivery>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }
}

// ---------------------------------------------------------------------------
// MockTimecardRepository
// ---------------------------------------------------------------------------

pub struct MockTimecardRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockTimecardRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl TimecardRepository for MockTimecardRepository {
    async fn create_card(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
        _card_id: &str,
        _label: Option<&str>,
    ) -> Result<TimecardCard, sqlx::Error> {
        check_fail!(self);
        todo!("MockTimecardRepository::create_card")
    }

    async fn list_cards(
        &self,
        _tenant_id: Uuid,
        _employee_id: Option<Uuid>,
    ) -> Result<Vec<TimecardCard>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn get_card(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn get_card_by_card_id(
        &self,
        _tenant_id: Uuid,
        _card_id: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn delete_card(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(false)
    }

    async fn find_card_by_card_id(
        &self,
        _tenant_id: Uuid,
        _card_id: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn find_employee_id_by_nfc(
        &self,
        _tenant_id: Uuid,
        _nfc_id: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn create_punch(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
        _device_id: Option<Uuid>,
    ) -> Result<TimePunch, sqlx::Error> {
        check_fail!(self);
        todo!("MockTimecardRepository::create_punch")
    }

    async fn get_employee_name(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
    ) -> Result<String, sqlx::Error> {
        check_fail!(self);
        Ok(String::new())
    }

    async fn list_today_punches(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
    ) -> Result<Vec<TimePunch>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn count_punches(
        &self,
        _tenant_id: Uuid,
        _employee_id: Option<Uuid>,
        _date_from: Option<DateTime<Utc>>,
        _date_to: Option<DateTime<Utc>>,
    ) -> Result<i64, sqlx::Error> {
        check_fail!(self);
        Ok(0)
    }

    async fn list_punches(
        &self,
        _tenant_id: Uuid,
        _employee_id: Option<Uuid>,
        _date_from: Option<DateTime<Utc>>,
        _date_to: Option<DateTime<Utc>>,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<TimePunchWithDevice>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn list_punches_for_csv(
        &self,
        _tenant_id: Uuid,
        _employee_id: Option<Uuid>,
        _date_from: Option<DateTime<Utc>>,
        _date_to: Option<DateTime<Utc>>,
    ) -> Result<Vec<TimePunchCsvRow>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }
}

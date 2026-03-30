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
    pub tag_data: std::sync::Mutex<Option<NfcTag>>,
    pub car_inspection_json: std::sync::Mutex<Option<serde_json::Value>>,
    pub delete_returns_true: AtomicBool,
}

impl Default for MockNfcTagRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            tag_data: std::sync::Mutex::new(None),
            car_inspection_json: std::sync::Mutex::new(None),
            delete_returns_true: AtomicBool::new(false),
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
        Ok(self.tag_data.lock().unwrap().clone())
    }

    async fn get_car_inspection_json(
        &self,
        _tenant_id: Uuid,
        _car_inspection_id: i32,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        check_fail!(self);
        Ok(self.car_inspection_json.lock().unwrap().clone())
    }

    async fn list(
        &self,
        _tenant_id: Uuid,
        _car_inspection_id: Option<i32>,
    ) -> Result<Vec<NfcTag>, sqlx::Error> {
        check_fail!(self);
        let data = self.tag_data.lock().unwrap();
        Ok(data.iter().cloned().collect())
    }

    async fn register(
        &self,
        _tenant_id: Uuid,
        nfc_uuid: &str,
        car_inspection_id: i32,
    ) -> Result<NfcTag, sqlx::Error> {
        check_fail!(self);
        Ok(NfcTag {
            id: 1,
            nfc_uuid: nfc_uuid.to_string(),
            car_inspection_id,
            created_at: Utc::now(),
        })
    }

    async fn delete(&self, _tenant_id: Uuid, _nfc_uuid: &str) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(self.delete_returns_true.load(Ordering::SeqCst))
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
    pub users: std::sync::Mutex<Vec<UserRow>>,
}

impl Default for MockTenantUsersRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            users: std::sync::Mutex::new(vec![]),
        }
    }
}

#[async_trait::async_trait]
impl TenantUsersRepository for MockTenantUsersRepository {
    async fn list_users(&self, _tenant_id: Uuid) -> Result<Vec<UserRow>, sqlx::Error> {
        check_fail!(self);
        Ok(self.users.lock().unwrap().clone())
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
        Ok(TenantAllowedEmail {
            id: Uuid::new_v4(),
            tenant_id: _tenant_id,
            email: _email.to_string(),
            role: _role.to_string(),
            created_at: chrono::Utc::now(),
        })
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
    /// true にすると register_driver / record_tenko が Some を返す (成功パス)
    pub return_some: AtomicBool,
    /// true にすると list_numbers / list_drivers がサンプルデータを返す
    pub return_data: AtomicBool,
}

impl Default for MockTenkoCallRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            return_some: AtomicBool::new(false),
            return_data: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl TenkoCallRepository for MockTenkoCallRepository {
    async fn register_driver(
        &self,
        call_number: &str,
        _phone_number: &str,
        _driver_name: &str,
        _employee_code: Option<&str>,
    ) -> Result<Option<RegisterDriverResult>, sqlx::Error> {
        check_fail!(self);
        if self.return_some.load(Ordering::SeqCst) {
            Ok(Some(RegisterDriverResult {
                driver_id: 42,
                call_number: Some(call_number.to_string()),
            }))
        } else {
            Ok(None)
        }
    }

    async fn record_tenko(
        &self,
        _phone_number: &str,
        _driver_name: &str,
        _latitude: f64,
        _longitude: f64,
    ) -> Result<Option<DriverInfo>, sqlx::Error> {
        check_fail!(self);
        if self.return_some.load(Ordering::SeqCst) {
            Ok(Some(DriverInfo {
                id: 42,
                call_number: Some("090-1234-5678".to_string()),
                tenant_id: "test-tenant".to_string(),
            }))
        } else {
            Ok(None)
        }
    }

    async fn list_numbers(&self) -> Result<Vec<TenkoCallNumberRow>, sqlx::Error> {
        check_fail!(self);
        if self.return_data.load(Ordering::SeqCst) {
            Ok(vec![TenkoCallNumberRow {
                id: 1,
                call_number: "090-0000-0001".to_string(),
                tenant_id: "test-tenant".to_string(),
                label: Some("Office A".to_string()),
                created_at: "2026-01-01 00:00:00".to_string(),
            }])
        } else {
            Ok(vec![])
        }
    }

    async fn create_number(
        &self,
        _call_number: &str,
        _tenant_id: &str,
        _label: Option<&str>,
    ) -> Result<i32, sqlx::Error> {
        check_fail!(self);
        Ok(99)
    }

    async fn delete_number(&self, _id: i32) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }

    async fn list_drivers(&self) -> Result<Vec<TenkoCallDriverRow>, sqlx::Error> {
        check_fail!(self);
        if self.return_data.load(Ordering::SeqCst) {
            Ok(vec![TenkoCallDriverRow {
                id: 1,
                phone_number: "080-1111-2222".to_string(),
                driver_name: "Test Driver".to_string(),
                call_number: Some("090-0000-0001".to_string()),
                tenant_id: "test-tenant".to_string(),
                employee_code: Some("EMP001".to_string()),
                created_at: "2026-01-01 00:00:00".to_string(),
            }])
        } else {
            Ok(vec![])
        }
    }
}

// ---------------------------------------------------------------------------
// MockTenkoRecordsRepository
// ---------------------------------------------------------------------------

pub struct MockTenkoRecordsRepository {
    pub fail_next: AtomicBool,
    pub return_some: AtomicBool,
    pub return_data: AtomicBool,
    pub return_ng_data: AtomicBool,
}

impl Default for MockTenkoRecordsRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            return_some: AtomicBool::new(false),
            return_data: AtomicBool::new(false),
            return_ng_data: AtomicBool::new(false),
        }
    }
}

fn make_mock_tenko_record_for_list(tenant_id: Uuid, id: Uuid) -> TenkoRecord {
    TenkoRecord {
        id,
        tenant_id,
        session_id: Uuid::new_v4(),
        employee_id: Uuid::new_v4(),
        tenko_type: "pre_operation".to_string(),
        status: "completed".to_string(),
        record_data: serde_json::json!({}),
        employee_name: "Test Employee".to_string(),
        responsible_manager_name: "Manager A".to_string(),
        tenko_method: "face_to_face".to_string(),
        location: Some("Tokyo Office".to_string()),
        alcohol_result: Some("negative".to_string()),
        alcohol_value: Some(0.0),
        alcohol_has_face_photo: true,
        temperature: Some(36.5),
        systolic: Some(120),
        diastolic: Some(80),
        pulse: Some(72),
        instruction: Some("Drive safely".to_string()),
        instruction_confirmed_at: Some(Utc::now()),
        report_vehicle_road_status: Some("good".to_string()),
        report_driver_alternation: Some("none".to_string()),
        report_no_report: Some(false),
        report_vehicle_road_audio_url: None,
        report_driver_alternation_audio_url: None,
        started_at: Some(Utc::now()),
        completed_at: Some(Utc::now()),
        recorded_at: Utc::now(),
        record_hash: "abc123hash".to_string(),
        self_declaration: Some(
            serde_json::json!({"illness": false, "fatigue": false, "sleep_deprivation": false}),
        ),
        safety_judgment: Some(serde_json::json!({"status": "pass", "failed_items": []})),
        daily_inspection: Some(
            serde_json::json!({"brakes": "ok", "tires": "ok", "lights": "ok", "steering": "ok", "wipers": "ok", "mirrors": "ok", "horn": "ok", "seatbelts": "ok"}),
        ),
        interrupted_at: None,
        resumed_at: None,
        resume_reason: None,
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
        if self.return_data.load(Ordering::SeqCst) || self.return_ng_data.load(Ordering::SeqCst) {
            return Ok(1);
        }
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
        if self.return_ng_data.load(Ordering::SeqCst) {
            let mut record = make_mock_tenko_record_for_list(_tenant_id, Uuid::new_v4());
            record.daily_inspection = Some(
                serde_json::json!({"brakes": "ng", "tires": "ok", "lights": "ok", "steering": "ok", "wipers": "ok", "mirrors": "ok", "horn": "ok", "seatbelts": "ok"}),
            );
            return Ok(vec![record]);
        }
        if self.return_data.load(Ordering::SeqCst) {
            return Ok(vec![make_mock_tenko_record_for_list(
                _tenant_id,
                Uuid::new_v4(),
            )]);
        }
        Ok(vec![])
    }

    async fn get(&self, _tenant_id: Uuid, _id: Uuid) -> Result<Option<TenkoRecord>, sqlx::Error> {
        check_fail!(self);
        if self.return_some.load(Ordering::SeqCst) {
            return Ok(Some(make_mock_tenko_record_for_list(_tenant_id, _id)));
        }
        Ok(None)
    }

    async fn list_all(
        &self,
        _tenant_id: Uuid,
        _filter: &TenkoRecordFilter,
    ) -> Result<Vec<TenkoRecord>, sqlx::Error> {
        check_fail!(self);
        if self.return_ng_data.load(Ordering::SeqCst) {
            let mut record = make_mock_tenko_record_for_list(_tenant_id, Uuid::new_v4());
            record.daily_inspection = Some(
                serde_json::json!({"brakes": "ng", "tires": "ok", "lights": "ok", "steering": "ok", "wipers": "ok", "mirrors": "ok", "horn": "ok", "seatbelts": "ok"}),
            );
            return Ok(vec![record]);
        }
        if self.return_data.load(Ordering::SeqCst) {
            return Ok(vec![make_mock_tenko_record_for_list(
                _tenant_id,
                Uuid::new_v4(),
            )]);
        }
        Ok(vec![])
    }
}

// ---------------------------------------------------------------------------
// MockTenkoSchedulesRepository
// ---------------------------------------------------------------------------

pub struct MockTenkoSchedulesRepository {
    pub fail_next: AtomicBool,
    pub return_none: AtomicBool,
}

impl Default for MockTenkoSchedulesRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            return_none: AtomicBool::new(false),
        }
    }
}

fn make_mock_schedule(
    tenant_id: Uuid,
    employee_id: Uuid,
    tenko_type: &str,
    instruction: Option<String>,
) -> TenkoSchedule {
    TenkoSchedule {
        id: Uuid::new_v4(),
        tenant_id,
        employee_id,
        tenko_type: tenko_type.to_string(),
        responsible_manager_name: "Manager".to_string(),
        scheduled_at: Utc::now(),
        instruction,
        consumed: false,
        consumed_by_session_id: None,
        overdue_notified_at: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn make_mock_schedule_with_id(id: Uuid, tenant_id: Uuid) -> TenkoSchedule {
    TenkoSchedule {
        id,
        tenant_id,
        employee_id: Uuid::new_v4(),
        tenko_type: "pre_operation".to_string(),
        responsible_manager_name: "Manager".to_string(),
        scheduled_at: Utc::now(),
        instruction: Some("Test instruction".to_string()),
        consumed: false,
        consumed_by_session_id: None,
        overdue_notified_at: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

#[async_trait::async_trait]
impl TenkoSchedulesRepository for MockTenkoSchedulesRepository {
    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateTenkoSchedule,
    ) -> Result<TenkoSchedule, sqlx::Error> {
        check_fail!(self);
        Ok(make_mock_schedule(
            tenant_id,
            input.employee_id,
            &input.tenko_type,
            input.instruction.clone(),
        ))
    }

    async fn batch_create(
        &self,
        tenant_id: Uuid,
        inputs: &[CreateTenkoSchedule],
    ) -> Result<Vec<TenkoSchedule>, sqlx::Error> {
        check_fail!(self);
        Ok(inputs
            .iter()
            .map(|s| {
                make_mock_schedule(
                    tenant_id,
                    s.employee_id,
                    &s.tenko_type,
                    s.instruction.clone(),
                )
            })
            .collect())
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

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<TenkoSchedule>, sqlx::Error> {
        check_fail!(self);
        if self.return_none.load(Ordering::SeqCst) {
            return Ok(None);
        }
        Ok(Some(make_mock_schedule_with_id(id, tenant_id)))
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        _input: &UpdateTenkoSchedule,
    ) -> Result<Option<TenkoSchedule>, sqlx::Error> {
        check_fail!(self);
        if self.return_none.load(Ordering::SeqCst) {
            return Ok(None);
        }
        Ok(Some(make_mock_schedule_with_id(id, tenant_id)))
    }

    async fn delete(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        if self.return_none.load(Ordering::SeqCst) {
            return Ok(false);
        }
        Ok(true)
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
    /// Controls the status of the session returned by get()
    pub session_status: std::sync::Mutex<String>,
    /// Controls the tenko_type of the session returned by get()
    pub session_tenko_type: std::sync::Mutex<String>,
    /// Controls the employee_id of sessions returned by get()
    pub session_employee_id: std::sync::Mutex<Uuid>,
    /// When true, get() returns Some session; when false, returns None
    pub return_session: AtomicBool,
    /// When true, get_schedule_unconsumed returns a schedule
    pub return_schedule: AtomicBool,
    /// Employee ID set on the schedule (for mismatch tests)
    pub schedule_employee_id: std::sync::Mutex<Uuid>,
    /// When true, get_employee_name returns Some
    pub return_employee_name: AtomicBool,
    /// When true, get_schedule_instruction returns Some instruction
    pub return_instruction: AtomicBool,
    /// Controls carrying items count
    pub carrying_items_count: std::sync::Mutex<i64>,
    /// Controls daily_inspection on session (for resume logic)
    pub session_has_daily_inspection: AtomicBool,
    /// Controls self_declaration on session (for resume logic)
    pub session_has_self_declaration: AtomicBool,
}

impl Default for MockTenkoSessionRepository {
    fn default() -> Self {
        let emp_id = Uuid::new_v4();
        Self {
            fail_next: AtomicBool::new(false),
            session_status: std::sync::Mutex::new("identity_verified".to_string()),
            session_tenko_type: std::sync::Mutex::new("pre_operation".to_string()),
            session_employee_id: std::sync::Mutex::new(emp_id),
            return_session: AtomicBool::new(true),
            return_schedule: AtomicBool::new(true),
            schedule_employee_id: std::sync::Mutex::new(emp_id),
            return_employee_name: AtomicBool::new(true),
            return_instruction: AtomicBool::new(false),
            carrying_items_count: std::sync::Mutex::new(0),
            session_has_daily_inspection: AtomicBool::new(false),
            session_has_self_declaration: AtomicBool::new(false),
        }
    }
}

fn make_mock_session(
    tenant_id: Uuid,
    id: Uuid,
    employee_id: Uuid,
    status: &str,
    tenko_type: &str,
    has_daily_inspection: bool,
    has_self_declaration: bool,
) -> TenkoSession {
    let now = Utc::now();
    TenkoSession {
        id,
        tenant_id,
        employee_id,
        schedule_id: Some(Uuid::new_v4()),
        tenko_type: tenko_type.to_string(),
        status: status.to_string(),
        identity_verified_at: Some(now),
        identity_face_photo_url: None,
        measurement_id: None,
        alcohol_result: None,
        alcohol_value: None,
        alcohol_tested_at: None,
        alcohol_face_photo_url: None,
        temperature: Some(36.5),
        systolic: Some(120),
        diastolic: Some(80),
        pulse: Some(72),
        medical_measured_at: Some(now),
        medical_manual_input: None,
        instruction_confirmed_at: None,
        report_vehicle_road_status: None,
        report_driver_alternation: None,
        report_no_report: None,
        report_vehicle_road_audio_url: None,
        report_driver_alternation_audio_url: None,
        report_submitted_at: None,
        location: None,
        responsible_manager_name: Some("Manager".to_string()),
        cancel_reason: None,
        interrupted_at: None,
        resumed_at: None,
        resume_reason: None,
        resumed_by_user_id: None,
        self_declaration: if has_self_declaration {
            Some(
                serde_json::json!({"illness": false, "fatigue": false, "sleep_deprivation": false, "declared_at": now}),
            )
        } else {
            None
        },
        safety_judgment: None,
        daily_inspection: if has_daily_inspection {
            Some(serde_json::json!({"brakes": "ok"}))
        } else {
            None
        },
        carrying_items_checked: None,
        started_at: Some(now),
        completed_at: None,
        created_at: now,
        updated_at: now,
    }
}

fn make_mock_tenko_record(tenant_id: Uuid, session: &TenkoSession) -> TenkoRecord {
    let now = Utc::now();
    TenkoRecord {
        id: Uuid::new_v4(),
        tenant_id,
        session_id: session.id,
        employee_id: session.employee_id,
        tenko_type: session.tenko_type.clone(),
        status: session.status.clone(),
        record_data: serde_json::to_value(session).unwrap_or_default(),
        employee_name: "Test Employee".to_string(),
        responsible_manager_name: session.responsible_manager_name.clone().unwrap_or_default(),
        tenko_method: "face".to_string(),
        location: session.location.clone(),
        alcohol_result: session.alcohol_result.clone(),
        alcohol_value: session.alcohol_value,
        alcohol_has_face_photo: false,
        temperature: session.temperature,
        systolic: session.systolic,
        diastolic: session.diastolic,
        pulse: session.pulse,
        instruction: None,
        instruction_confirmed_at: session.instruction_confirmed_at,
        report_vehicle_road_status: session.report_vehicle_road_status.clone(),
        report_driver_alternation: session.report_driver_alternation.clone(),
        report_no_report: session.report_no_report,
        report_vehicle_road_audio_url: session.report_vehicle_road_audio_url.clone(),
        report_driver_alternation_audio_url: session.report_driver_alternation_audio_url.clone(),
        started_at: session.started_at,
        completed_at: session.completed_at,
        recorded_at: now,
        record_hash: "mock_hash".to_string(),
        self_declaration: session.self_declaration.clone(),
        safety_judgment: session.safety_judgment.clone(),
        daily_inspection: session.daily_inspection.clone(),
        interrupted_at: session.interrupted_at,
        resumed_at: session.resumed_at,
        resume_reason: session.resume_reason.clone(),
    }
}

#[async_trait::async_trait]
impl TenkoSessionRepository for MockTenkoSessionRepository {
    async fn get(&self, _tenant_id: Uuid, _id: Uuid) -> Result<Option<TenkoSession>, sqlx::Error> {
        check_fail!(self);
        if !self.return_session.load(Ordering::SeqCst) {
            return Ok(None);
        }
        let status = self.session_status.lock().unwrap().clone();
        let tenko_type = self.session_tenko_type.lock().unwrap().clone();
        let employee_id = *self.session_employee_id.lock().unwrap();
        let has_di = self.session_has_daily_inspection.load(Ordering::SeqCst);
        let has_sd = self.session_has_self_declaration.load(Ordering::SeqCst);
        Ok(Some(make_mock_session(
            _tenant_id,
            _id,
            employee_id,
            &status,
            &tenko_type,
            has_di,
            has_sd,
        )))
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
        if !self.return_schedule.load(Ordering::SeqCst) {
            return Ok(None);
        }
        let employee_id = *self.schedule_employee_id.lock().unwrap();
        let tenko_type = self.session_tenko_type.lock().unwrap().clone();
        Ok(Some(TenkoSchedule {
            id: _schedule_id,
            tenant_id: _tenant_id,
            employee_id,
            tenko_type,
            responsible_manager_name: "Manager".to_string(),
            scheduled_at: Utc::now(),
            instruction: Some("Test instruction".to_string()),
            consumed: false,
            consumed_by_session_id: None,
            overdue_notified_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }))
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
        if self.return_instruction.load(Ordering::SeqCst) {
            Ok(Some("Test instruction".to_string()))
        } else {
            Ok(None)
        }
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
        Ok(make_mock_session(
            _tenant_id,
            Uuid::new_v4(),
            _employee_id,
            _initial_status,
            _tenko_type,
            false,
            false,
        ))
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
        let employee_id = *self.session_employee_id.lock().unwrap();
        let tenko_type = self.session_tenko_type.lock().unwrap().clone();
        let mut session = make_mock_session(
            _tenant_id,
            _id,
            employee_id,
            _next_status,
            &tenko_type,
            false,
            false,
        );
        session.alcohol_result = Some(_alcohol_result.to_string());
        session.alcohol_value = Some(_alcohol_value);
        session.cancel_reason = _cancel_reason.clone();
        session.completed_at = _completed_at;
        Ok(session)
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
        let employee_id = *self.session_employee_id.lock().unwrap();
        let mut session = make_mock_session(
            _tenant_id,
            _id,
            employee_id,
            "self_declaration_pending",
            "pre_operation",
            false,
            false,
        );
        session.temperature = _temperature;
        session.systolic = _systolic;
        session.diastolic = _diastolic;
        session.pulse = _pulse;
        Ok(session)
    }

    async fn confirm_instruction(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<TenkoSession, sqlx::Error> {
        check_fail!(self);
        let employee_id = *self.session_employee_id.lock().unwrap();
        let tenko_type = self.session_tenko_type.lock().unwrap().clone();
        let mut session = make_mock_session(
            _tenant_id,
            _id,
            employee_id,
            "completed",
            &tenko_type,
            false,
            false,
        );
        session.instruction_confirmed_at = Some(Utc::now());
        session.completed_at = Some(Utc::now());
        Ok(session)
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
        let employee_id = *self.session_employee_id.lock().unwrap();
        let mut session = make_mock_session(
            _tenant_id,
            _id,
            employee_id,
            _next_status,
            "post_operation",
            false,
            false,
        );
        session.report_vehicle_road_status = Some(_vehicle_road_status.to_string());
        session.report_driver_alternation = Some(_driver_alternation.to_string());
        session.completed_at = _completed_at;
        Ok(session)
    }

    async fn cancel(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _reason: &Option<String>,
    ) -> Result<TenkoSession, sqlx::Error> {
        check_fail!(self);
        let employee_id = *self.session_employee_id.lock().unwrap();
        let tenko_type = self.session_tenko_type.lock().unwrap().clone();
        let mut session = make_mock_session(
            _tenant_id,
            _id,
            employee_id,
            "cancelled",
            &tenko_type,
            false,
            false,
        );
        session.cancel_reason = _reason.clone();
        session.completed_at = Some(Utc::now());
        Ok(session)
    }

    async fn update_self_declaration(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _declaration_json: &serde_json::Value,
    ) -> Result<TenkoSession, sqlx::Error> {
        check_fail!(self);
        let employee_id = *self.session_employee_id.lock().unwrap();
        let mut session = make_mock_session(
            _tenant_id,
            _id,
            employee_id,
            "self_declaration_pending",
            "pre_operation",
            false,
            false,
        );
        session.self_declaration = Some(_declaration_json.clone());
        Ok(session)
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
        let employee_id = *self.session_employee_id.lock().unwrap();
        let mut session = make_mock_session(
            _tenant_id,
            _id,
            employee_id,
            _next_status,
            "pre_operation",
            false,
            false,
        );
        session.safety_judgment = Some(_judgment_json.clone());
        session.interrupted_at = _interrupted_at;
        Ok(session)
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
        let employee_id = *self.session_employee_id.lock().unwrap();
        let mut session = make_mock_session(
            _tenant_id,
            _id,
            employee_id,
            _next_status,
            "pre_operation",
            true,
            false,
        );
        session.daily_inspection = Some(_inspection_json.clone());
        session.cancel_reason = _cancel_reason.clone();
        session.completed_at = _completed_at;
        Ok(session)
    }

    async fn update_carrying_items(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _carrying_json: &serde_json::Value,
    ) -> Result<TenkoSession, sqlx::Error> {
        check_fail!(self);
        let employee_id = *self.session_employee_id.lock().unwrap();
        let mut session = make_mock_session(
            _tenant_id,
            _id,
            employee_id,
            "identity_verified",
            "pre_operation",
            true,
            false,
        );
        session.carrying_items_checked = Some(_carrying_json.clone());
        Ok(session)
    }

    async fn interrupt(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _reason: &Option<String>,
    ) -> Result<TenkoSession, sqlx::Error> {
        check_fail!(self);
        let employee_id = *self.session_employee_id.lock().unwrap();
        let tenko_type = self.session_tenko_type.lock().unwrap().clone();
        let mut session = make_mock_session(
            _tenant_id,
            _id,
            employee_id,
            "interrupted",
            &tenko_type,
            false,
            false,
        );
        session.interrupted_at = Some(Utc::now());
        Ok(session)
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
        let employee_id = *self.session_employee_id.lock().unwrap();
        let tenko_type = self.session_tenko_type.lock().unwrap().clone();
        let mut session = make_mock_session(
            _tenant_id,
            _id,
            employee_id,
            _resume_to,
            &tenko_type,
            false,
            false,
        );
        session.resumed_at = Some(Utc::now());
        session.resume_reason = Some(_reason.to_string());
        session.resumed_by_user_id = _resumed_by_user_id;
        Ok(session)
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
        Ok(*self.carrying_items_count.lock().unwrap())
    }

    async fn get_employee_name(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
    ) -> Result<Option<String>, sqlx::Error> {
        check_fail!(self);
        if self.return_employee_name.load(Ordering::SeqCst) {
            Ok(Some("Test Employee".to_string()))
        } else {
            Ok(None)
        }
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
        Ok(make_mock_tenko_record(_tenant_id, _session))
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
    /// When true, `get` returns Some and `delete` returns true.
    pub return_found: AtomicBool,
}

impl Default for MockTenkoWebhooksRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            return_found: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl TenkoWebhooksRepository for MockTenkoWebhooksRepository {
    async fn upsert(
        &self,
        tenant_id: Uuid,
        input: &CreateWebhookConfig,
    ) -> Result<WebhookConfig, sqlx::Error> {
        check_fail!(self);
        let now = Utc::now();
        Ok(WebhookConfig {
            id: Uuid::new_v4(),
            tenant_id,
            event_type: input.event_type.clone(),
            url: input.url.clone(),
            secret: input.secret.clone(),
            enabled: input.enabled,
            created_at: now,
            updated_at: now,
        })
    }

    async fn list(&self, _tenant_id: Uuid) -> Result<Vec<WebhookConfig>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<WebhookConfig>, sqlx::Error> {
        check_fail!(self);
        if self.return_found.load(Ordering::SeqCst) {
            let now = Utc::now();
            Ok(Some(WebhookConfig {
                id,
                tenant_id,
                event_type: "tenko_completed".to_string(),
                url: "https://example.com/hook".to_string(),
                secret: None,
                enabled: true,
                created_at: now,
                updated_at: now,
            }))
        } else {
            Ok(None)
        }
    }

    async fn delete(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        if self.return_found.load(Ordering::SeqCst) {
            Ok(true)
        } else {
            Ok(false)
        }
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
    /// Controls create_card: when true, returns a card; when false, returns conflict error
    pub create_card_conflict: AtomicBool,
    /// Controls get_card / get_card_by_card_id: when set, returns Some
    pub card_data: std::sync::Mutex<Option<TimecardCard>>,
    /// Controls delete_card: when true, returns true (deleted)
    pub delete_returns_true: AtomicBool,
    /// Controls find_card_by_card_id for punch: when set, returns Some
    pub find_card_data: std::sync::Mutex<Option<TimecardCard>>,
    /// Controls find_employee_id_by_nfc for punch fallback
    pub nfc_employee_id: std::sync::Mutex<Option<Uuid>>,
    /// Employee name returned by get_employee_name
    pub employee_name: std::sync::Mutex<String>,
    /// CSV rows returned by list_punches_for_csv
    pub csv_rows: std::sync::Mutex<Vec<TimePunchCsvRow>>,
    /// Cards returned by list_cards
    pub cards_list: std::sync::Mutex<Vec<TimecardCard>>,
}

impl Default for MockTimecardRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            create_card_conflict: AtomicBool::new(false),
            card_data: std::sync::Mutex::new(None),
            delete_returns_true: AtomicBool::new(false),
            find_card_data: std::sync::Mutex::new(None),
            nfc_employee_id: std::sync::Mutex::new(None),
            employee_name: std::sync::Mutex::new(String::new()),
            csv_rows: std::sync::Mutex::new(vec![]),
            cards_list: std::sync::Mutex::new(vec![]),
        }
    }
}

#[async_trait::async_trait]
impl TimecardRepository for MockTimecardRepository {
    async fn create_card(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        card_id: &str,
        label: Option<&str>,
    ) -> Result<TimecardCard, sqlx::Error> {
        check_fail!(self);
        if self.create_card_conflict.load(Ordering::SeqCst) {
            return Err(sqlx::Error::Database(Box::new(MockDbErrorC(
                "duplicate key value violates unique constraint \"idx_timecard_cards_unique\""
                    .to_string(),
            ))));
        }
        Ok(TimecardCard {
            id: Uuid::new_v4(),
            tenant_id,
            employee_id,
            card_id: card_id.to_string(),
            label: label.map(|s| s.to_string()),
            created_at: Utc::now(),
        })
    }

    async fn list_cards(
        &self,
        _tenant_id: Uuid,
        _employee_id: Option<Uuid>,
    ) -> Result<Vec<TimecardCard>, sqlx::Error> {
        check_fail!(self);
        Ok(self.cards_list.lock().unwrap().clone())
    }

    async fn get_card(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        check_fail!(self);
        Ok(self.card_data.lock().unwrap().clone())
    }

    async fn get_card_by_card_id(
        &self,
        _tenant_id: Uuid,
        _card_id: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        check_fail!(self);
        Ok(self.card_data.lock().unwrap().clone())
    }

    async fn delete_card(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(self.delete_returns_true.load(Ordering::SeqCst))
    }

    async fn find_card_by_card_id(
        &self,
        _tenant_id: Uuid,
        _card_id: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        check_fail!(self);
        Ok(self.find_card_data.lock().unwrap().clone())
    }

    async fn find_employee_id_by_nfc(
        &self,
        _tenant_id: Uuid,
        _nfc_id: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        check_fail!(self);
        Ok(*self.nfc_employee_id.lock().unwrap())
    }

    async fn create_punch(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        device_id: Option<Uuid>,
    ) -> Result<TimePunch, sqlx::Error> {
        check_fail!(self);
        Ok(TimePunch {
            id: Uuid::new_v4(),
            tenant_id,
            employee_id,
            device_id,
            punched_at: Utc::now(),
            created_at: Utc::now(),
        })
    }

    async fn get_employee_name(
        &self,
        _tenant_id: Uuid,
        _employee_id: Uuid,
    ) -> Result<String, sqlx::Error> {
        check_fail!(self);
        Ok(self.employee_name.lock().unwrap().clone())
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
        Ok(self.csv_rows.lock().unwrap().clone())
    }
}

/// Helper for creating database errors with custom messages
struct MockDbErrorC(String);

impl std::fmt::Debug for MockDbErrorC {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MockDbErrorC({})", self.0)
    }
}

impl std::fmt::Display for MockDbErrorC {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for MockDbErrorC {}

impl sqlx::error::DatabaseError for MockDbErrorC {
    fn message(&self) -> &str {
        &self.0
    }

    fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
        self
    }

    fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
        self
    }

    fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
        self
    }

    fn kind(&self) -> sqlx::error::ErrorKind {
        sqlx::error::ErrorKind::UniqueViolation
    }
}

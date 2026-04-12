use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use ts_rs::TS;
use uuid::Uuid;

// --- Tenant ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Tenant {
    pub id: Uuid,
    pub name: String,
    pub slug: Option<String>,
    pub email_domain: Option<String>,
    pub created_at: DateTime<Utc>,
}

// --- Tenant Allowed Email (招待) ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export)]
pub struct TenantAllowedEmail {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub email: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
}

// --- Employee ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Employee {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub code: Option<String>,
    pub nfc_id: Option<String>,
    pub name: String,
    pub face_photo_url: Option<String>,
    #[serde(skip_serializing)]
    pub face_embedding: Option<Vec<f64>>,
    pub face_embedding_at: Option<DateTime<Utc>>,
    pub face_model_version: Option<String>,
    pub face_approval_status: String,
    pub face_approved_by: Option<Uuid>,
    pub face_approved_at: Option<DateTime<Utc>>,
    pub license_issue_date: Option<chrono::NaiveDate>,
    pub license_expiry_date: Option<chrono::NaiveDate>,
    pub role: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateLicense {
    pub license_issue_date: Option<chrono::NaiveDate>,
    pub license_expiry_date: Option<chrono::NaiveDate>,
}

#[derive(Debug, Deserialize)]
pub struct CreateEmployee {
    pub code: Option<String>,
    pub nfc_id: Option<String>,
    pub name: String,
    #[serde(default = "default_driver")]
    pub role: Vec<String>,
}

fn default_driver() -> Vec<String> {
    vec!["driver".to_string()]
}

#[derive(Debug, Deserialize)]
pub struct UpdateFace {
    pub face_photo_url: Option<String>,
    pub face_embedding: Option<Vec<f64>>,
    pub face_model_version: Option<String>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct FaceDataEntry {
    pub id: Uuid,
    pub face_embedding: Option<Vec<f64>>,
    pub face_embedding_at: Option<DateTime<Utc>>,
    pub face_model_version: Option<String>,
    pub face_approval_status: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateNfcId {
    pub nfc_id: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEmployee {
    pub name: String,
    pub code: Option<String>,
    pub role: Option<Vec<String>>,
}

// --- User ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub google_sub: Option<String>,
    pub lineworks_id: Option<String>,
    pub line_user_id: Option<String>,
    pub email: String,
    pub name: String,
    pub role: String,
    pub username: Option<String>,
    pub password_hash: Option<String>,
    pub refresh_token_hash: Option<String>,
    pub refresh_token_expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

// --- Measurement ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Measurement {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub employee_id: Uuid,
    #[serde(rename = "alcohol_value")]
    pub alcohol_level: Option<f64>,
    #[serde(rename = "result_type")]
    pub result: Option<String>,
    pub device_use_count: i32,
    pub face_photo_url: Option<String>,
    pub video_url: Option<String>,
    pub measured_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub status: String,
    // Medical data (BLE Medical Gateway)
    pub temperature: Option<f64>,
    pub systolic: Option<i32>,
    pub diastolic: Option<i32>,
    pub pulse: Option<i32>,
    pub medical_measured_at: Option<DateTime<Utc>>,
    pub face_verified: Option<bool>,
    pub medical_manual_input: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CreateMeasurement {
    pub employee_id: Uuid,
    #[serde(alias = "alcohol_level")]
    pub alcohol_value: f64,
    #[serde(alias = "result")]
    pub result_type: String,
    pub face_photo_url: Option<String>,
    pub video_url: Option<String>,
    pub measured_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub device_use_count: Option<i32>,
    // Medical data (BLE Medical Gateway)
    pub temperature: Option<f64>,
    pub systolic: Option<i32>,
    pub diastolic: Option<i32>,
    pub pulse: Option<i32>,
    pub medical_measured_at: Option<DateTime<Utc>>,
    pub face_verified: Option<bool>,
    pub medical_manual_input: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct StartMeasurement {
    pub employee_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMeasurement {
    pub status: Option<String>,
    #[serde(alias = "alcohol_level")]
    pub alcohol_value: Option<f64>,
    #[serde(alias = "result")]
    pub result_type: Option<String>,
    pub face_photo_url: Option<String>,
    pub video_url: Option<String>,
    pub measured_at: Option<DateTime<Utc>>,
    pub device_use_count: Option<i32>,
    pub temperature: Option<f64>,
    pub systolic: Option<i32>,
    pub diastolic: Option<i32>,
    pub pulse: Option<i32>,
    pub medical_measured_at: Option<DateTime<Utc>>,
    pub face_verified: Option<bool>,
    pub medical_manual_input: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct MeasurementFilter {
    pub employee_id: Option<Uuid>,
    #[serde(alias = "result")]
    pub result_type: Option<String>,
    #[serde(alias = "from")]
    pub date_from: Option<DateTime<Utc>>,
    #[serde(alias = "to")]
    pub date_to: Option<DateTime<Utc>>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MeasurementsResponse {
    pub measurements: Vec<Measurement>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

// --- Tenko Schedule (点呼実施予定) ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TenkoSchedule {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub employee_id: Uuid,
    pub tenko_type: String,
    pub responsible_manager_name: String,
    pub scheduled_at: DateTime<Utc>,
    pub instruction: Option<String>,
    pub consumed: bool,
    pub consumed_by_session_id: Option<Uuid>,
    pub overdue_notified_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTenkoSchedule {
    pub employee_id: Uuid,
    pub tenko_type: String,
    pub responsible_manager_name: String,
    pub scheduled_at: DateTime<Utc>,
    pub instruction: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BatchCreateTenkoSchedules {
    pub schedules: Vec<CreateTenkoSchedule>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTenkoSchedule {
    pub responsible_manager_name: Option<String>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub instruction: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TenkoScheduleFilter {
    pub employee_id: Option<Uuid>,
    pub tenko_type: Option<String>,
    pub consumed: Option<bool>,
    pub date_from: Option<DateTime<Utc>>,
    pub date_to: Option<DateTime<Utc>>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct TenkoSchedulesResponse {
    pub schedules: Vec<TenkoSchedule>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

// --- Tenko Session (点呼セッション) ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TenkoSession {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub employee_id: Uuid,
    pub schedule_id: Option<Uuid>,
    pub tenko_type: String,
    pub status: String,
    pub identity_verified_at: Option<DateTime<Utc>>,
    pub identity_face_photo_url: Option<String>,
    pub measurement_id: Option<Uuid>,
    pub alcohol_result: Option<String>,
    pub alcohol_value: Option<f64>,
    pub alcohol_tested_at: Option<DateTime<Utc>>,
    pub alcohol_face_photo_url: Option<String>,
    pub temperature: Option<f64>,
    pub systolic: Option<i32>,
    pub diastolic: Option<i32>,
    pub pulse: Option<i32>,
    pub medical_measured_at: Option<DateTime<Utc>>,
    pub medical_manual_input: Option<bool>,
    pub instruction_confirmed_at: Option<DateTime<Utc>>,
    pub report_vehicle_road_status: Option<String>,
    pub report_driver_alternation: Option<String>,
    pub report_no_report: Option<bool>,
    pub report_vehicle_road_audio_url: Option<String>,
    pub report_driver_alternation_audio_url: Option<String>,
    pub report_submitted_at: Option<DateTime<Utc>>,
    pub location: Option<String>,
    pub responsible_manager_name: Option<String>,
    pub cancel_reason: Option<String>,
    pub interrupted_at: Option<DateTime<Utc>>,
    pub resumed_at: Option<DateTime<Utc>>,
    pub resume_reason: Option<String>,
    pub resumed_by_user_id: Option<Uuid>,
    // Phase 2
    pub self_declaration: Option<serde_json::Value>,
    pub safety_judgment: Option<serde_json::Value>,
    pub daily_inspection: Option<serde_json::Value>,
    pub carrying_items_checked: Option<serde_json::Value>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct StartTenkoSession {
    pub schedule_id: Option<Uuid>,  // remote mode では None
    pub tenko_type: Option<String>, // schedule なしの場合に使用 (default: "pre_operation")
    pub employee_id: Uuid,
    pub identity_face_photo_url: Option<String>,
    pub location: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitAlcoholResult {
    pub measurement_id: Option<Uuid>,
    pub alcohol_result: String,
    pub alcohol_value: f64,
    pub alcohol_face_photo_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitMedicalData {
    pub temperature: Option<f64>,
    pub systolic: Option<i32>,
    pub diastolic: Option<i32>,
    pub pulse: Option<i32>,
    pub medical_measured_at: Option<DateTime<Utc>>,
    pub medical_manual_input: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitOperationReport {
    pub vehicle_road_status: String,
    pub driver_alternation: String,
    pub vehicle_road_audio_url: Option<String>,
    pub driver_alternation_audio_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CancelTenkoSession {
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TenkoSessionFilter {
    pub employee_id: Option<Uuid>,
    pub status: Option<String>,
    pub tenko_type: Option<String>,
    pub date_from: Option<DateTime<Utc>>,
    pub date_to: Option<DateTime<Utc>>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct TenkoSessionsResponse {
    pub sessions: Vec<TenkoSession>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

// --- Tenko Record (点呼記録 — 不変) ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TenkoRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub session_id: Uuid,
    pub employee_id: Uuid,
    pub tenko_type: String,
    pub status: String,
    pub record_data: serde_json::Value,
    pub employee_name: String,
    pub responsible_manager_name: String,
    pub tenko_method: String,
    pub location: Option<String>,
    pub alcohol_result: Option<String>,
    pub alcohol_value: Option<f64>,
    pub alcohol_has_face_photo: bool,
    pub temperature: Option<f64>,
    pub systolic: Option<i32>,
    pub diastolic: Option<i32>,
    pub pulse: Option<i32>,
    pub instruction: Option<String>,
    pub instruction_confirmed_at: Option<DateTime<Utc>>,
    pub report_vehicle_road_status: Option<String>,
    pub report_driver_alternation: Option<String>,
    pub report_no_report: Option<bool>,
    pub report_vehicle_road_audio_url: Option<String>,
    pub report_driver_alternation_audio_url: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub recorded_at: DateTime<Utc>,
    pub record_hash: String,
    // Phase 2
    pub self_declaration: Option<serde_json::Value>,
    pub safety_judgment: Option<serde_json::Value>,
    pub daily_inspection: Option<serde_json::Value>,
    pub interrupted_at: Option<DateTime<Utc>>,
    pub resumed_at: Option<DateTime<Utc>>,
    pub resume_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TenkoRecordFilter {
    pub employee_id: Option<Uuid>,
    pub tenko_type: Option<String>,
    pub status: Option<String>,
    pub date_from: Option<DateTime<Utc>>,
    pub date_to: Option<DateTime<Utc>>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct TenkoRecordsResponse {
    pub records: Vec<TenkoRecord>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

// --- Webhook ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WebhookConfig {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub event_type: String,
    pub url: String,
    #[serde(skip_serializing)]
    pub secret: Option<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateWebhookConfig {
    pub event_type: String,
    pub url: String,
    pub secret: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WebhookDelivery {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub config_id: Uuid,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub status_code: Option<i32>,
    pub response_body: Option<String>,
    pub attempt: i32,
    pub delivered_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub success: bool,
}

// --- Tenko Dashboard ---

#[derive(Debug, Serialize)]
pub struct TenkoDashboard {
    pub pending_schedules: i64,
    pub active_sessions: i64,
    pub interrupted_sessions: i64,
    pub completed_today: i64,
    pub cancelled_today: i64,
    pub overdue_schedules: Vec<TenkoSchedule>,
}

// --- Timecard ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TimecardCard {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub employee_id: Uuid,
    pub card_id: String,
    pub label: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTimecardCard {
    pub employee_id: Uuid,
    pub card_id: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TimePunch {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub employee_id: Uuid,
    pub device_id: Option<Uuid>,
    pub punched_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTimePunchByCard {
    pub card_id: String,
    pub device_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct TimePunchWithEmployee {
    pub punch: TimePunch,
    pub employee_name: String,
    pub today_punches: Vec<TimePunch>,
}

#[derive(Debug, Deserialize)]
pub struct TimePunchFilter {
    pub employee_id: Option<Uuid>,
    pub date_from: Option<DateTime<Utc>>,
    pub date_to: Option<DateTime<Utc>>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct TimePunchWithDevice {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub employee_id: Uuid,
    pub device_id: Option<Uuid>,
    pub device_name: Option<String>,
    pub employee_name: Option<String>,
    pub punched_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct TimePunchesResponse {
    pub punches: Vec<TimePunchWithDevice>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

// --- Phase 2: Health Baselines (要件7) ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EmployeeHealthBaseline {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub employee_id: Uuid,
    pub baseline_systolic: i32,
    pub baseline_diastolic: i32,
    pub baseline_temperature: f64,
    pub systolic_tolerance: i32,
    pub diastolic_tolerance: i32,
    pub temperature_tolerance: f64,
    pub measurement_validity_minutes: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateHealthBaseline {
    pub employee_id: Uuid,
    pub baseline_systolic: Option<i32>,
    pub baseline_diastolic: Option<i32>,
    pub baseline_temperature: Option<f64>,
    pub systolic_tolerance: Option<i32>,
    pub diastolic_tolerance: Option<i32>,
    pub temperature_tolerance: Option<f64>,
    pub measurement_validity_minutes: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateHealthBaseline {
    pub baseline_systolic: Option<i32>,
    pub baseline_diastolic: Option<i32>,
    pub baseline_temperature: Option<f64>,
    pub systolic_tolerance: Option<i32>,
    pub diastolic_tolerance: Option<i32>,
    pub temperature_tolerance: Option<f64>,
    pub measurement_validity_minutes: Option<i32>,
}

// --- Phase 2: Self-Declaration (要件8) ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfDeclaration {
    pub illness: bool,
    pub fatigue: bool,
    pub sleep_deprivation: bool,
    pub declared_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitSelfDeclaration {
    pub illness: bool,
    pub fatigue: bool,
    pub sleep_deprivation: bool,
}

// --- Phase 2: Safety Judgment (要件9) ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyJudgment {
    pub status: String,
    pub failed_items: Vec<String>,
    pub judged_at: DateTime<Utc>,
    pub medical_diffs: Option<MedicalDiffs>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedicalDiffs {
    pub systolic_diff: Option<i32>,
    pub diastolic_diff: Option<i32>,
    pub temperature_diff: Option<f64>,
}

// --- Phase 2: Daily Inspection (要件11) ---

#[derive(Debug, Deserialize)]
pub struct SubmitDailyInspection {
    pub brakes: String,
    pub tires: String,
    pub lights: String,
    pub steering: String,
    pub wipers: String,
    pub mirrors: String,
    pub horn: String,
    pub seatbelts: String,
}

// --- Phase 2: Interrupt/Resume (要件10) ---

#[derive(Debug, Deserialize)]
pub struct InterruptSession {
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ResumeSession {
    pub reason: String,
}

// --- Phase 2: Equipment Failures (要件17) ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EquipmentFailure {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub failure_type: String,
    pub description: String,
    pub affected_device: Option<String>,
    pub detected_at: DateTime<Utc>,
    pub detected_by: Option<String>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolution_notes: Option<String>,
    pub session_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateEquipmentFailure {
    pub failure_type: String,
    pub description: String,
    pub affected_device: Option<String>,
    pub detected_at: Option<DateTime<Utc>>,
    pub detected_by: Option<String>,
    pub session_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEquipmentFailure {
    pub resolution_notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EquipmentFailureFilter {
    pub failure_type: Option<String>,
    pub resolved: Option<bool>,
    pub session_id: Option<Uuid>,
    pub date_from: Option<DateTime<Utc>>,
    pub date_to: Option<DateTime<Utc>>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct EquipmentFailuresResponse {
    pub failures: Vec<EquipmentFailure>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

// --- Dtako: Office ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DtakoOffice {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub office_cd: String,
    pub office_name: String,
}

// --- Dtako: Vehicle ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DtakoVehicle {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub vehicle_cd: String,
    pub vehicle_name: String,
}

// --- Dtako: Event Classification ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DtakoEventClassification {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub event_cd: String,
    pub event_name: String,
    pub classification: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDtakoClassification {
    pub classification: String,
}

// --- Dtako: Operation (KUDGURI) ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DtakoOperation {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub unko_no: String,
    pub crew_role: i32,
    pub reading_date: chrono::NaiveDate,
    pub operation_date: Option<chrono::NaiveDate>,
    pub office_id: Option<Uuid>,
    pub vehicle_id: Option<Uuid>,
    pub driver_id: Option<Uuid>,
    pub departure_at: Option<DateTime<Utc>>,
    pub return_at: Option<DateTime<Utc>>,
    pub garage_out_at: Option<DateTime<Utc>>,
    pub garage_in_at: Option<DateTime<Utc>>,
    pub meter_start: Option<f64>,
    pub meter_end: Option<f64>,
    pub total_distance: Option<f64>,
    pub drive_time_general: Option<i32>,
    pub drive_time_highway: Option<i32>,
    pub drive_time_bypass: Option<i32>,
    pub safety_score: Option<f64>,
    pub economy_score: Option<f64>,
    pub total_score: Option<f64>,
    pub raw_data: serde_json::Value,
    pub r2_key_prefix: Option<String>,
    pub uploaded_at: DateTime<Utc>,
    pub has_kudgivt: bool,
}

#[derive(Debug, Serialize, FromRow)]
pub struct DtakoOperationListItem {
    pub id: Uuid,
    pub unko_no: String,
    pub crew_role: i32,
    pub reading_date: chrono::NaiveDate,
    pub operation_date: Option<chrono::NaiveDate>,
    pub driver_name: Option<String>,
    pub vehicle_name: Option<String>,
    pub total_distance: Option<f64>,
    pub safety_score: Option<f64>,
    pub economy_score: Option<f64>,
    pub total_score: Option<f64>,
    pub has_kudgivt: bool,
}

#[derive(Debug, Deserialize)]
pub struct DtakoOperationFilter {
    pub date_from: Option<chrono::NaiveDate>,
    pub date_to: Option<chrono::NaiveDate>,
    pub driver_cd: Option<String>,
    pub vehicle_cd: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct DtakoOperationsResponse {
    pub operations: Vec<DtakoOperationListItem>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

// --- Dtako: Upload History ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DtakoUploadHistory {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub uploaded_by: Option<Uuid>,
    pub filename: String,
    pub operations_count: i32,
    pub r2_zip_key: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

// --- Dtako: Daily Work Hours ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DtakoDailyWorkHours {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub driver_id: Uuid,
    pub work_date: chrono::NaiveDate,
    pub start_time: chrono::NaiveTime,
    pub total_work_minutes: Option<i32>,
    pub total_drive_minutes: Option<i32>,
    pub total_rest_minutes: Option<i32>,
    pub late_night_minutes: i32,
    pub drive_minutes: i32,
    pub cargo_minutes: i32,
    pub overlap_drive_minutes: i32,
    pub overlap_cargo_minutes: i32,
    pub overlap_break_minutes: i32,
    pub overlap_restraint_minutes: i32,
    pub ot_late_night_minutes: i32,
    pub total_distance: Option<f64>,
    pub operation_count: i32,
    pub unko_nos: Option<Vec<String>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct DtakoDailyHoursFilter {
    pub driver_id: Option<Uuid>,
    pub date_from: Option<chrono::NaiveDate>,
    pub date_to: Option<chrono::NaiveDate>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct DtakoDailyHoursResponse {
    pub items: Vec<DtakoDailyWorkHours>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

// --- Dtako: Daily Work Segments ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DtakoDailyWorkSegment {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub driver_id: Uuid,
    pub work_date: chrono::NaiveDate,
    pub unko_no: String,
    pub segment_index: i32,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub work_minutes: i32,
    pub labor_minutes: i32,
    pub late_night_minutes: i32,
    pub drive_minutes: i32,
    pub cargo_minutes: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct DtakoSegmentsResponse {
    pub segments: Vec<DtakoDailyWorkSegment>,
}

// --- NFC Tag ---

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize, ts_rs::TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct NfcTag {
    pub id: i32,
    pub nfc_uuid: String,
    pub car_inspection_id: i32,
    pub created_at: DateTime<Utc>,
}

// --- Carrying Items ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CarryingItem {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub item_name: String,
    pub is_required: bool,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateCarryingItem {
    pub item_name: String,
    pub is_required: Option<bool>,
    pub sort_order: Option<i32>,
    #[serde(default)]
    pub vehicle_conditions: Vec<VehicleConditionInput>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCarryingItem {
    pub item_name: Option<String>,
    pub is_required: Option<bool>,
    pub sort_order: Option<i32>,
    pub vehicle_conditions: Option<Vec<VehicleConditionInput>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CarryingItemVehicleCondition {
    pub id: Uuid,
    pub carrying_item_id: Uuid,
    pub category: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VehicleConditionInput {
    pub category: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CarryingItemCheck {
    pub id: Uuid,
    pub session_id: Uuid,
    pub item_id: Uuid,
    pub item_name: String,
    pub checked: bool,
    pub checked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitCarryingItemCheck {
    pub item_id: Uuid,
    pub checked: bool,
}

#[derive(Debug, Deserialize)]
pub struct SubmitCarryingItemChecks {
    pub checks: Vec<SubmitCarryingItemCheck>,
}

// --- Guidance Records ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct GuidanceRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub employee_id: Uuid,
    pub guidance_type: String,
    pub title: String,
    pub content: String,
    pub guided_by: Option<String>,
    pub guided_at: DateTime<Utc>,
    pub parent_id: Option<Uuid>,
    pub depth: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateGuidanceRecord {
    pub employee_id: Uuid,
    pub guidance_type: Option<String>,
    pub title: String,
    pub content: Option<String>,
    pub guided_by: Option<String>,
    pub guided_at: Option<DateTime<Utc>>,
    pub parent_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct GuidanceRecordAttachment {
    pub id: Uuid,
    pub record_id: Uuid,
    pub file_name: String,
    pub file_type: String,
    pub file_size: Option<i32>,
    pub storage_url: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateGuidanceRecord {
    pub guidance_type: Option<String>,
    pub title: Option<String>,
    pub content: Option<String>,
    pub guided_by: Option<String>,
    pub guided_at: Option<DateTime<Utc>>,
}

// --- Communication Items ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CommunicationItem {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub title: String,
    pub content: String,
    pub priority: String,
    pub target_employee_id: Option<Uuid>,
    pub is_active: bool,
    pub effective_from: Option<DateTime<Utc>>,
    pub effective_until: Option<DateTime<Utc>>,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateCommunicationItem {
    pub title: String,
    pub content: Option<String>,
    pub priority: Option<String>,
    pub target_employee_id: Option<Uuid>,
    pub effective_from: Option<DateTime<Utc>>,
    pub effective_until: Option<DateTime<Utc>>,
    pub created_by: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCommunicationItem {
    pub title: Option<String>,
    pub content: Option<String>,
    pub priority: Option<String>,
    pub target_employee_id: Option<Uuid>,
    pub is_active: Option<bool>,
    pub effective_from: Option<DateTime<Utc>>,
    pub effective_until: Option<DateTime<Utc>>,
}

// --- Dtako Logs (リアルタイム車両GPS) ---

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DtakologRow {
    pub gps_direction: f64,
    pub gps_latitude: f64,
    pub gps_longitude: f64,
    pub vehicle_cd: i32,
    pub vehicle_name: String,
    pub driver_name: Option<String>,
    pub address_disp_c: Option<String>,
    pub data_date_time: String,
    pub address_disp_p: Option<String>,
    pub sub_driver_cd: i32,
    pub all_state: Option<String>,
    pub recive_type_color_name: Option<String>,
    pub all_state_ex: Option<String>,
    pub state2: Option<String>,
    pub all_state_font_color: Option<String>,
    pub speed: f32,
}

/// フロントエンド互換の PascalCase JSON レスポンス
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct DtakologView {
    #[serde(rename = "GPSDirection")]
    pub gps_direction: f64,
    #[serde(rename = "GPSLatitude")]
    pub gps_latitude: f64,
    #[serde(rename = "GPSLongitude")]
    pub gps_longitude: f64,
    #[serde(rename = "VehicleCD")]
    pub vehicle_cd: i32,
    pub vehicle_name: String,
    pub driver_name: Option<String>,
    #[serde(rename = "AddressDispC")]
    pub address_disp_c: Option<String>,
    pub data_date_time: String,
    #[serde(rename = "AddressDispP")]
    pub address_disp_p: Option<String>,
    #[serde(rename = "SubDriverCD")]
    pub sub_driver_cd: i32,
    pub all_state: String,
    pub recive_type_color_name: Option<String>,
    pub all_state_ex: Option<String>,
    pub state2: String,
    pub all_state_font_color: Option<String>,
    pub speed: serde_json::Value,
}

impl From<DtakologRow> for DtakologView {
    fn from(r: DtakologRow) -> Self {
        let all_state = r.all_state.unwrap_or_default();
        let state2 = if ["Drive", "Rest", "Break"].contains(&all_state.as_str()) {
            r.state2.unwrap_or_default()
        } else {
            String::new()
        };
        let speed: serde_json::Value = if r.speed == 0.0 {
            serde_json::Value::String(String::new())
        } else {
            // f32→f64 変換時の精度ノイズを除去 (74.9000015258789 → 74.9)
            let rounded = (r.speed as f64 * 10.0).round() / 10.0;
            serde_json::json!(rounded)
        };
        Self {
            gps_direction: r.gps_direction,
            gps_latitude: r.gps_latitude,
            gps_longitude: r.gps_longitude,
            vehicle_cd: r.vehicle_cd,
            vehicle_name: r.vehicle_name,
            driver_name: r.driver_name,
            address_disp_c: r.address_disp_c,
            data_date_time: r.data_date_time,
            address_disp_p: r.address_disp_p,
            sub_driver_cd: r.sub_driver_cd,
            all_state,
            recive_type_color_name: r.recive_type_color_name,
            all_state_ex: r.all_state_ex,
            state2,
            all_state_font_color: r.all_state_font_color,
            speed,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct DtakologDateQuery {
    pub date_time: String,
    pub vehicle_cd: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct DtakologDateRangeQuery {
    pub start_date_time: String,
    pub end_date_time: String,
    pub vehicle_cd: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct DtakologSelectQuery {
    pub address_disp_p: Option<String>,
    pub branch_cd: Option<i32>,
    pub vehicle_cds: Option<String>,
}

/// POST /dtako-logs/bulk リクエストボディ (PascalCase JSON)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DtakologInput {
    // PK fields (DataDateTime は null 許容 — スクレイパーが GPS 未取得車両で null を送る)
    pub data_date_time: Option<String>,
    #[serde(rename = "VehicleCD")]
    pub vehicle_cd: i32,

    // Required fields with defaults
    #[serde(rename = "__type", default)]
    pub r#type: String,
    #[serde(default)]
    pub all_state_font_color_index: i32,
    #[serde(default = "default_transparent")]
    pub all_state_ryout_color: String,
    #[serde(rename = "BranchCD", default)]
    pub branch_cd: i32,
    #[serde(default)]
    pub branch_name: String,
    #[serde(rename = "CurrentWorkCD", default)]
    pub current_work_cd: i32,
    #[serde(default)]
    pub data_filter_type: i32,
    #[serde(default)]
    pub disp_flag: i32,
    #[serde(rename = "DriverCD", default)]
    pub driver_cd: i32,
    #[serde(rename = "GPSDirection", default)]
    pub gps_direction: f64,
    #[serde(rename = "GPSEnable", default)]
    pub gps_enable: i32,
    #[serde(rename = "GPSLatitude", default)]
    pub gps_latitude: f64,
    #[serde(rename = "GPSLongitude", default)]
    pub gps_longitude: f64,
    #[serde(rename = "GPSSatelliteNum", default)]
    pub gps_satellite_num: i32,
    #[serde(default)]
    pub operation_state: i32,
    #[serde(default)]
    pub recive_event_type: i32,
    #[serde(default)]
    pub recive_packet_type: i32,
    #[serde(rename = "ReciveWorkCD", default)]
    pub recive_work_cd: i32,
    #[serde(default)]
    pub revo: i32,
    #[serde(default)]
    pub setting_temp: String,
    #[serde(default)]
    pub setting_temp1: String,
    #[serde(default)]
    pub setting_temp3: String,
    #[serde(default)]
    pub setting_temp4: String,
    #[serde(default)]
    pub speed: f32,
    #[serde(rename = "SubDriverCD", default)]
    pub sub_driver_cd: i32,
    #[serde(default)]
    pub temp_state: i32,
    #[serde(default)]
    pub vehicle_name: String,

    // Optional fields
    #[serde(rename = "AddressDispC")]
    pub address_disp_c: Option<String>,
    #[serde(rename = "AddressDispP")]
    pub address_disp_p: Option<String>,
    pub all_state: Option<String>,
    pub all_state_ex: Option<String>,
    pub all_state_font_color: Option<String>,
    pub comu_date_time: Option<String>,
    pub current_work_name: Option<String>,
    pub driver_name: Option<String>,
    pub event_val: Option<String>,
    #[serde(rename = "GPSLatiAndLong")]
    pub gps_lati_and_long: Option<String>,
    #[serde(rename = "ODOMeter")]
    pub odometer: Option<String>,
    pub recive_type_color_name: Option<String>,
    pub recive_type_name: Option<String>,
    pub start_work_date_time: Option<String>,
    pub state: Option<String>,
    pub state1: Option<String>,
    pub state2: Option<String>,
    pub state3: Option<String>,
    pub state_flag: Option<String>,
    pub temp1: Option<String>,
    pub temp2: Option<String>,
    pub temp3: Option<String>,
    pub temp4: Option<String>,
    pub vehicle_icon_color: Option<String>,
    pub vehicle_icon_label_for_datetime: Option<String>,
    pub vehicle_icon_label_for_driver: Option<String>,
    pub vehicle_icon_label_for_vehicle: Option<String>,
}

fn default_transparent() -> String {
    "Transparent".to_string()
}

/// POST /dtako-logs/bulk レスポンス
#[derive(Debug, Serialize)]
pub struct BulkUpsertResponse {
    pub success: bool,
    pub records_added: i32,
    pub total_records: i32,
    pub message: String,
}

#[cfg(test)]
mod dtakolog_tests {
    use super::*;

    fn make_row(all_state: Option<&str>, state2: Option<&str>, speed: f32) -> DtakologRow {
        DtakologRow {
            gps_direction: 180.0,
            gps_latitude: 35123456.0,
            gps_longitude: 139123456.0,
            vehicle_cd: 1,
            vehicle_name: "Truck-1".into(),
            driver_name: Some("Driver A".into()),
            address_disp_c: Some("Tokyo".into()),
            data_date_time: "26/04/04 10:00".into(),
            address_disp_p: Some("Shibuya".into()),
            sub_driver_cd: 0,
            all_state: all_state.map(String::from),
            recive_type_color_name: None,
            all_state_ex: None,
            state2: state2.map(String::from),
            all_state_font_color: None,
            speed,
        }
    }

    #[test]
    fn speed_zero_becomes_empty_string() {
        let view = DtakologView::from(make_row(Some("Drive"), None, 0.0));
        assert_eq!(view.speed, serde_json::Value::String(String::new()));
    }

    #[test]
    fn speed_nonzero_becomes_number() {
        let view = DtakologView::from(make_row(Some("Drive"), None, 60.5));
        assert_eq!(view.speed, serde_json::json!(60.5));
    }

    #[test]
    fn state2_populated_when_drive() {
        let view = DtakologView::from(make_row(Some("Drive"), Some("SubState"), 0.0));
        assert_eq!(view.state2, "SubState");
    }

    #[test]
    fn state2_populated_when_rest() {
        let view = DtakologView::from(make_row(Some("Rest"), Some("Resting"), 0.0));
        assert_eq!(view.state2, "Resting");
    }

    #[test]
    fn state2_populated_when_break() {
        let view = DtakologView::from(make_row(Some("Break"), Some("OnBreak"), 0.0));
        assert_eq!(view.state2, "OnBreak");
    }

    #[test]
    fn state2_empty_when_other_state() {
        let view = DtakologView::from(make_row(Some("End"), Some("ShouldNotAppear"), 0.0));
        assert_eq!(view.state2, "");
    }

    #[test]
    fn state2_empty_when_no_all_state() {
        let view = DtakologView::from(make_row(None, Some("ShouldNotAppear"), 0.0));
        assert_eq!(view.state2, "");
    }

    #[test]
    fn all_state_defaults_to_empty_when_none() {
        let view = DtakologView::from(make_row(None, None, 0.0));
        assert_eq!(view.all_state, "");
    }

    #[test]
    fn json_keys_are_pascal_case() {
        let view = DtakologView::from(make_row(Some("Drive"), None, 50.0));
        let json = serde_json::to_value(&view).unwrap();
        assert!(json.get("GPSDirection").is_some());
        assert!(json.get("GPSLatitude").is_some());
        assert!(json.get("GPSLongitude").is_some());
        assert!(json.get("VehicleCD").is_some());
        assert!(json.get("VehicleName").is_some());
        assert!(json.get("DataDateTime").is_some());
        assert!(json.get("SubDriverCD").is_some());
        assert!(json.get("AddressDispC").is_some());
        assert!(json.get("AddressDispP").is_some());
        assert!(json.get("AllState").is_some());
        assert!(json.get("State2").is_some());
        assert!(json.get("Speed").is_some());
    }
}

// --- Items (物品管理) ---

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Item {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub owner_type: String,
    pub owner_user_id: Option<Uuid>,
    pub item_type: String,
    pub name: String,
    pub barcode: String,
    pub category: String,
    pub description: String,
    pub image_url: String,
    pub url: String,
    pub quantity: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateItem {
    pub parent_id: Option<Uuid>,
    pub owner_type: Option<String>,
    pub owner_user_id: Option<Uuid>,
    pub item_type: Option<String>,
    pub name: String,
    pub barcode: Option<String>,
    pub category: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub url: Option<String>,
    pub quantity: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateItem {
    pub name: Option<String>,
    pub barcode: Option<String>,
    pub category: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub url: Option<String>,
    pub quantity: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ItemFile {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// --- Trouble ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export)]
pub struct TroubleWorkflowState {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub label: String,
    pub color: String,
    pub sort_order: i32,
    pub is_initial: bool,
    pub is_terminal: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateWorkflowState {
    pub name: String,
    pub label: String,
    pub color: Option<String>,
    pub sort_order: Option<i32>,
    pub is_initial: Option<bool>,
    pub is_terminal: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export)]
pub struct TroubleWorkflowTransition {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub from_state_id: Uuid,
    pub to_state_id: Uuid,
    pub label: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateWorkflowTransition {
    pub from_state_id: Uuid,
    pub to_state_id: Uuid,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export)]
pub struct TroubleTicket {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub ticket_no: i32,
    pub category: String,
    pub title: String,
    pub occurred_at: Option<DateTime<Utc>>,
    pub occurred_date: Option<chrono::NaiveDate>,
    pub company_name: String,
    pub office_name: String,
    pub department: String,
    pub person_name: String,
    pub person_id: Option<Uuid>,
    pub vehicle_number: String,
    pub registration_number: String,
    pub location: String,
    pub description: String,
    pub status_id: Option<Uuid>,
    pub assigned_to: Option<Uuid>,
    pub progress_notes: String,
    pub allowance: String,
    pub damage_amount: Option<String>,
    pub compensation_amount: Option<String>,
    pub confirmation_notice: String,
    pub disciplinary_content: String,
    pub disciplinary_action: String,
    pub road_service_cost: Option<String>,
    pub counterparty: String,
    pub counterparty_insurance: String,
    pub custom_fields: serde_json::Value,
    pub due_date: Option<DateTime<Utc>>,
    pub overdue_notified_at: Option<DateTime<Utc>>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateTroubleTicket {
    pub category: String,
    pub title: Option<String>,
    pub occurred_at: Option<DateTime<Utc>>,
    pub occurred_date: Option<chrono::NaiveDate>,
    pub company_name: Option<String>,
    pub office_name: Option<String>,
    pub department: Option<String>,
    pub person_name: Option<String>,
    pub person_id: Option<Uuid>,
    pub vehicle_number: Option<String>,
    pub location: Option<String>,
    pub description: Option<String>,
    pub assigned_to: Option<Uuid>,
    pub damage_amount: Option<f64>,
    pub compensation_amount: Option<f64>,
    pub road_service_cost: Option<f64>,
    pub counterparty: Option<String>,
    pub counterparty_insurance: Option<String>,
    pub custom_fields: Option<serde_json::Value>,
    pub due_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct UpdateTroubleTicket {
    pub category: Option<String>,
    pub title: Option<String>,
    pub occurred_at: Option<DateTime<Utc>>,
    pub occurred_date: Option<chrono::NaiveDate>,
    pub company_name: Option<String>,
    pub office_name: Option<String>,
    pub department: Option<String>,
    pub person_name: Option<String>,
    pub person_id: Option<Uuid>,
    pub vehicle_number: Option<String>,
    pub location: Option<String>,
    pub description: Option<String>,
    pub assigned_to: Option<Uuid>,
    pub progress_notes: Option<String>,
    pub allowance: Option<String>,
    pub damage_amount: Option<f64>,
    pub compensation_amount: Option<f64>,
    pub confirmation_notice: Option<String>,
    pub disciplinary_content: Option<String>,
    pub disciplinary_action: Option<String>,
    pub road_service_cost: Option<f64>,
    pub counterparty: Option<String>,
    pub counterparty_insurance: Option<String>,
    pub custom_fields: Option<serde_json::Value>,
    pub due_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct TroubleTicketFilter {
    pub category: Option<String>,
    pub status_id: Option<Uuid>,
    pub person_name: Option<String>,
    pub company_name: Option<String>,
    pub office_name: Option<String>,
    pub date_from: Option<chrono::NaiveDate>,
    pub date_to: Option<chrono::NaiveDate>,
    pub q: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct TroubleTicketsResponse {
    pub tickets: Vec<TroubleTicket>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export)]
pub struct TroubleFile {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub ticket_id: Uuid,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub storage_key: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export)]
pub struct TroubleStatusHistory {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub ticket_id: Uuid,
    pub from_state_id: Option<Uuid>,
    pub to_state_id: Uuid,
    pub changed_by: Option<Uuid>,
    pub comment: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export)]
pub struct TroubleComment {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub ticket_id: Uuid,
    pub author_id: Option<Uuid>,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateTroubleComment {
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export)]
pub struct TroubleCategory {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateTroubleCategory {
    pub name: String,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export)]
pub struct TroubleOffice {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateTroubleOffice {
    pub name: String,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export)]
pub struct TroubleProgressStatus {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateTroubleProgressStatus {
    pub name: String,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct TransitionRequest {
    pub to_state_id: Uuid,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export)]
pub struct TroubleCustomFieldDef {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub field_key: String,
    pub label: String,
    pub field_type: String,
    pub options: Option<serde_json::Value>,
    pub required: bool,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateCustomFieldDef {
    pub field_key: String,
    pub label: String,
    pub field_type: String,
    pub options: Option<serde_json::Value>,
    pub required: Option<bool>,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export)]
pub struct TroubleNotificationPref {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub event_type: String,
    pub notify_channel: String,
    pub enabled: bool,
    pub recipient_ids: Vec<Uuid>,
    pub notify_admins: bool,
    pub lineworks_user_ids: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct UpsertNotificationPref {
    pub event_type: String,
    pub notify_channel: String,
    pub enabled: Option<bool>,
    pub recipient_ids: Option<Vec<Uuid>>,
    pub notify_admins: Option<bool>,
    pub lineworks_user_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export)]
pub struct TroubleSchedule {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub ticket_id: Uuid,
    pub scheduled_at: DateTime<Utc>,
    pub message: String,
    pub lineworks_user_ids: Vec<String>,
    pub cloud_task_name: Option<String>,
    pub status: String,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub sent_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateTroubleSchedule {
    pub ticket_id: Uuid,
    pub scheduled_at: DateTime<Utc>,
    pub message: String,
    pub lineworks_user_ids: Vec<String>,
}

// --- Trouble Tasks ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export)]
pub struct TroubleTask {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub ticket_id: Uuid,
    pub task_type: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub assigned_to: Option<Uuid>,
    pub due_date: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub sort_order: i32,
    pub next_action: String,
    pub next_action_by: Option<Uuid>,
    pub next_action_due: Option<DateTime<Utc>>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateTroubleTask {
    pub task_type: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub assigned_to: Option<Uuid>,
    #[serde(default)]
    pub due_date: Option<DateTime<Utc>>,
    #[serde(default)]
    pub sort_order: Option<i32>,
    #[serde(default)]
    pub next_action: Option<String>,
    #[serde(default)]
    pub next_action_by: Option<Uuid>,
    #[serde(default)]
    pub next_action_due: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct UpdateTroubleTask {
    #[serde(default)]
    pub task_type: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub assigned_to: Option<Option<Uuid>>,
    #[serde(default)]
    pub due_date: Option<Option<DateTime<Utc>>>,
    #[serde(default)]
    pub completed_at: Option<Option<DateTime<Utc>>>,
    #[serde(default)]
    pub sort_order: Option<i32>,
    #[serde(default)]
    pub next_action: Option<String>,
    #[serde(default)]
    pub next_action_by: Option<Option<Uuid>>,
    #[serde(default)]
    pub next_action_due: Option<Option<DateTime<Utc>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export)]
pub struct TroubleTaskActivity {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub task_id: Uuid,
    pub body: String,
    pub occurred_at: DateTime<Utc>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateTroubleTaskActivity {
    pub body: String,
    #[serde(default)]
    pub occurred_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct UpdateTroubleTaskActivity {
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub occurred_at: Option<Option<DateTime<Utc>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export)]
pub struct TroubleActivityFile {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub activity_id: Uuid,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub storage_key: String,
    pub created_at: DateTime<Utc>,
}

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
    pub email: String,
    pub name: String,
    pub role: String,
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

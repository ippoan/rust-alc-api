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
    /// メール ingest 等で使う 8 文字 hex の短縮 ID。NOT NULL + UNIQUE。
    pub short_id: String,
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

/// `PUT /api/employees/bulk-by-code` の 1 件分 (Refs ippoan/alc-app-s3#125)。
/// theearth の乗務員マスタを relay 経由で取り込む用途で、乗務員CD (code) を
/// キーに upsert する。
#[derive(Debug, Deserialize)]
pub struct EmployeeUpsertItem {
    pub code: String,
    pub name: String,
    pub nfc_id: Option<String>,
    pub license_issue_date: Option<chrono::NaiveDate>,
    pub license_expiry_date: Option<chrono::NaiveDate>,
}

#[derive(Debug, Deserialize)]
pub struct EmployeeBulkUpsert {
    pub items: Vec<EmployeeUpsertItem>,
}

/// upsert できなかった 1 件 (nfc_id 衝突 / INSERT 時の unique 違反)。
#[derive(Debug, Serialize)]
pub struct EmployeeUpsertSkipped {
    pub code: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct EmployeeUpsertSummary {
    pub created: usize,
    pub updated: usize,
    pub skipped: Vec<EmployeeUpsertSkipped>,
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

/// 打刻 1 件。**`hub_measurements` から導出する** (Refs ippoan/alc-app-s3#134) ので
/// `id` は `hub_measurements.id`。
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct TimePunchWithDevice {
    pub id: Uuid,
    pub tenant_id: Uuid,
    /// 解決できた社員。**未登録カードのタップでは None** — 行ごと落とすと
    /// 「タップしたのに履歴に出ない」になり、登録漏れに気付けなくなる
    pub employee_id: Option<Uuid>,
    /// 常に None。`devices(id)` への UUID FK だが、打刻端末の device_id は
    /// auth-worker 発行の文字列で入らない。どの端末かは `device_name` を見る
    pub device_id: Option<Uuid>,
    /// 打刻端末の `hub_measurements.device_id` (auth-worker 発行の文字列)
    pub device_name: Option<String>,
    /// `timecard` (打刻機のタップ) か `license` (点呼開始時の免許証)。
    /// **一覧は両方を返すので、区別はこの列でしかできない** — 始業点呼 = 始業打刻
    /// として同じ表に並べる運用のため (Refs ippoan/alc-app-s3#134)
    pub kind: String,
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

// --- Vehicle Settings Dump (R2 車輛設定 dump メタデータ) ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct VehicleSettingsDump {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub vehicle_cd: String,
    pub dump_dir: String,
    pub machine_id: Option<String>,
    pub firm_main_app: Option<String>,
    pub r2_json_key: String,
    pub r2_cfg_key: String,
    pub uploaded_at: DateTime<Utc>,
    pub uploaded_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
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
    /// 車輌CD (`dtako_vehicles.vehicle_cd`)。一番星 (CAPE#01) の車番との突合キーに使う
    /// (Refs ohishi-exp/nuxt-dtako-admin#198 Phase 8)。
    pub vehicle_cd: Option<String>,
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

// ============================================================
// dtako_tickets — dtako (デジタコ) エラー通知メールの起票テーブル
// Refs: ippoan/email-receiver#1 / ippoan/rust-alc-api#414
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export)]
pub struct DtakoTicket {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub source: String,
    pub source_email_subject: Option<String>,
    pub source_email_from: Option<String>,
    pub source_email_message_id: Option<String>,
    pub source_email_received_at: DateTime<Utc>,
    pub vehicle_name: String,
    pub vehicle_code: Option<String>,
    pub error_kind: String,
    pub status: String,
    pub comp_id: Option<String>,
    pub unko_no: Option<String>,
    pub operation_started_at: Option<DateTime<Utc>>,
    pub operation_ended_at: Option<DateTime<Utc>>,
    pub scraped_payload: Option<serde_json::Value>,
    pub settings_zip_r2_key: Option<String>,
    pub close_token: String,
    pub closed_at: Option<DateTime<Utc>>,
    pub closed_by: Option<String>,
    pub raw_email_text: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// email-receiver Worker → POST /api/dtako/tickets で起票するときの body。
/// 内部 shared-secret (X-Internal-Shared-Secret) + X-Tenant-ID で認証。
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct DtakoTicketCreate {
    #[serde(default = "default_source_email")]
    pub source: String,
    pub source_email_subject: Option<String>,
    pub source_email_from: Option<String>,
    pub source_email_message_id: Option<String>,
    pub source_email_received_at: DateTime<Utc>,
    pub vehicle_name: String,
    pub vehicle_code: Option<String>,
    pub error_kind: String,
    pub raw_email_text: Option<String>,
}

fn default_source_email() -> String {
    "email".to_string()
}

/// email-receiver Worker → PATCH /api/dtako/tickets/{id}/scraped で
/// F-VOS3020 scrape 結果を反映するときの body。null は変更なしを意味する。
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct DtakoTicketScrapedPatch {
    pub comp_id: Option<String>,
    pub unko_no: Option<String>,
    pub operation_started_at: Option<DateTime<Utc>>,
    pub operation_ended_at: Option<DateTime<Utc>>,
    pub settings_zip_r2_key: Option<String>,
    pub scraped_payload: Option<serde_json::Value>,
}

/// 一覧取得フィルタ (nuxt_dtako_logs から GET /api/dtako/tickets)。
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct DtakoTicketFilter {
    pub status: Option<String>,
    pub vehicle_name: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct DtakoTicketsResponse {
    pub tickets: Vec<DtakoTicket>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

/// browser (QR scan) → POST /api/dtako/tickets/close で close するときの body。
/// 認証は close_token のみ (URL-safe 32 byte hex)。
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct DtakoTicketCloseRequest {
    pub close_token: String,
    pub closed_by: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct DtakoTicketCloseResponse {
    pub ticket_id: Uuid,
}

// --- Hub measurements (CoreS3 ハブ ingest、Refs #564) ---

/// cf-alc-recorder Worker → POST /api/hub/measurements の 1 item。
/// 内部 shared-secret (X-Internal-Shared-Secret) + X-Tenant-ID で認証され、
/// tenant_id はヘッダー由来 (ペイロードに持たせない、#434 の教訓)。
/// device_id は cf-alc-recorder が introspect 済み device JWT の sub から注入する。
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct HubMeasurementCreate {
    pub device_id: String,
    /// temperature / blood_pressure / alcohol / fc1200_raw
    /// (allowlist は alc-devices hub_measurements::HUB_MEASUREMENT_KINDS)。
    pub kind: String,
    /// device 内シーケンス。UNIQUE (tenant_id, device_id, seq) で再送を冪等に吸収する。
    pub seq: i64,
    /// 端末計時 (unix ms)。時計未同期端末では null。
    pub recorded_at_ms: Option<i64>,
    /// 1 回の点呼を束ねる端末発番の識別子 (Refs ippoan/alc-app-s3#112)。
    /// 端末内でのみ一意で、グローバルには (tenant_id, device_id, session_id) の組。
    /// 点呼外の単発計測と旧ファームでは null (= セッション不明、欠損ではない)。
    #[serde(default)]
    pub session_id: Option<String>,
    /// ble-medical-gateway 互換 JSON をそのまま格納。
    pub payload: serde_json::Value,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct HubMeasurementsIngestResponse {
    /// 新規に insert された件数。
    pub inserted: i64,
    /// UNIQUE (tenant_id, device_id, seq) 衝突でスキップされた件数 (再送重複)。
    pub duplicates: i64,
}

/// `GET /api/hub/measurements` が返す 1 行 (Refs #592)。
/// `payload` は migration 126 のとおり JSONB をそのまま素通しする
/// (kind 別の型付けは別 issue)。
#[derive(Debug, Clone, Serialize, FromRow, TS)]
#[ts(export)]
pub struct HubMeasurement {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub device_id: String,
    /// temperature / blood_pressure / alcohol / fc1200_raw。
    pub kind: String,
    pub payload: serde_json::Value,
    pub seq: i64,
    /// 1 回の点呼を束ねる端末発番の識別子。点呼外の単発計測と旧データでは null。
    pub session_id: Option<String>,
    /// 端末計時。時計未同期端末では null。
    pub recorded_at: Option<DateTime<Utc>>,
    /// サーバ受信時刻。一覧の並び順・期間絞り込みはこの列が基準。
    pub created_at: DateTime<Utc>,
}

/// `GET /api/hub/measurements` のクエリ (Refs #592)。
///
/// `from` / `to` は **`created_at`** に対する閉区間。`recorded_at` は端末の時計
/// 未同期で NULL / ずれた値になり得るのに対し、`created_at` は必ず入っていて
/// 一覧の並び順とも一致するため、期間絞り込みの基準にはこちらを使う。
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export)]
pub struct HubMeasurementFilter {
    pub device_id: Option<String>,
    pub kind: Option<String>,
    /// 1 回の点呼で束ねて引くための識別子 (Refs ippoan/alc-app-s3#112)。
    pub session_id: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// `GET /api/hub/measurements` のレスポンス (Refs #592)。
///
/// 総件数 (COUNT(*)) は返さない。ingest テーブルは無制限に伸び続けるので
/// 毎回の全件 COUNT は index-only にならず重くなる。代わりに `limit + 1` 件を
/// 引いて `has_more` だけを返す (次ページの有無が分かれば UI は組める)。
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct HubMeasurementsListResponse {
    pub items: Vec<HubMeasurement>,
    /// 実際に適用された limit (clamp 後)。
    pub limit: i64,
    pub offset: i64,
    /// 次ページが存在するか。
    pub has_more: bool,
}

#[cfg(test)]
mod dtako_ticket_tests {
    use super::*;

    #[test]
    fn create_default_source_is_email() {
        // serde の `#[serde(default = "default_source_email")]` 経由で
        // body から `source` を省略した時のデフォルト値を検証する。
        let json = r#"{
            "source_email_received_at": "2026-06-15T08:00:00Z",
            "vehicle_name": "(16) 十勝800か16",
            "error_kind": "sd_card_error"
        }"#;
        let create: DtakoTicketCreate = serde_json::from_str(json).expect("parse");
        assert_eq!(create.source, "email");
        assert_eq!(create.vehicle_name, "(16) 十勝800か16");
        assert_eq!(create.error_kind, "sd_card_error");
        // 関数本体 (default_source_email) も直接 1 度呼ぶ。
        assert_eq!(default_source_email(), "email");
    }

    #[test]
    fn create_explicit_source_manual_overrides_default() {
        let json = r#"{
            "source": "manual",
            "source_email_received_at": "2026-06-15T08:00:00Z",
            "vehicle_name": "x",
            "error_kind": "sd_card_error"
        }"#;
        let create: DtakoTicketCreate = serde_json::from_str(json).expect("parse");
        assert_eq!(create.source, "manual");
    }
}

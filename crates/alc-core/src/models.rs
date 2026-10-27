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

// --- Dtako: Office ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DtakoOffice {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub office_cd: String,
    pub office_name: String,
}

// --- Dtako: Vehicle ---

// --- Dtako: Event Classification ---

// --- Dtako: Operation (KUDGURI) ---

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

// --- Dtako: Daily Work Segments ---

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

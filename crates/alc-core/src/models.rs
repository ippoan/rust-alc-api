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

// --- Timecard ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TimecardCard {
    pub id: Uuid,
    pub tenant_id: Uuid,
    /// 持ち主。**社員がまだ居ないカードは `None`** (Refs ippoan/rust-alc-api#644) —
    /// 社員マスタは別経路が別スケジュールで入れるので「カードが先、社員が後」が
    /// 普通に起きる。そのとき社員番号は `pending_employee_code` に残る。
    pub employee_id: Option<Uuid>,
    pub card_id: String,
    pub label: Option<String>,
    /// 行の**出所**。一括取り込み (`PUT /timecard/cards/bulk-by-code`) が入れた行だけに
    /// サーバ側の定数 (`CARD_SOURCE_LEDGER_SYNC`) が入り、それ以外は `NULL`
    /// (Refs ippoan/rust-alc-api#644)。
    ///
    /// **body から受け取らない。** 削除の射程 (`POST /timecard/cards/delete-by-card`) を
    /// 決める根拠なので、送り手が書ける値にすると alc 側で直接登録したカードまで
    /// 消せるようになる。`label` を根拠にできないのも同じ理由 (自由文で誰でも書ける)。
    pub source: Option<String>,
    /// 社員がまだ居ないカードの**社員番号** (Refs ippoan/rust-alc-api#644)。
    /// 結び付いている行は `NULL`。
    ///
    /// **受け入れたカードを後から必ず結び付けられる**ようにするための列で、
    /// 社員マスタ同期が同じ code の社員を入れた時点で `employee_id` が埋まり
    /// ここは `NULL` に戻る。一覧に出すのは「持ち主が空欄の行の理由」を
    /// 画面が説明できるようにするため (同じカードを別の社員に登録しようとすると
    /// `UNIQUE (tenant_id, card_id)` で 409 になるが、持ち主が空欄だと原因が読めない)。
    pub pending_employee_code: Option<String>,
    pub created_at: DateTime<Utc>,
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
    /// device 内シーケンス。unique index (tenant_id, device_id, seq, is_dev) で再送を冪等に吸収する
    /// (is_dev = dev端末の軸。列の既定値で入るので、この型には持たせない)。
    pub seq: i64,
    /// 端末計時 (unix ms)。時計未同期端末では null。
    pub recorded_at_ms: Option<i64>,
    /// 1 回の点呼を束ねる端末発番の識別子 (Refs ippoan/alc-app-s3#112)。
    /// 端末内でのみ一意で、グローバルには (tenant_id, device_id, session_id) の組。
    /// 点呼外の単発計測と旧ファームでは null (= セッション不明、欠損ではない)。
    #[serde(default)]
    pub session_id: Option<String>,
    /// CoreS3 が接続に使った hub の credential の device_id (Refs ippoan/alc-cam#2)。
    /// cf-alc-recorder が、hub の JWT と機体の鍵の証明が 1 回の接続で揃ったときだけ入れる
    /// (そのとき `device_id` は鍵の sub `alarm:<fingerprint>`)。鍵 ⇔ hub の結び付きはこの列から導く。
    /// 古い recorder・ブラウザ打刻では null。
    #[serde(default)]
    pub via_device_id: Option<String>,
    /// ble-medical-gateway 互換 JSON をそのまま格納。
    pub payload: serde_json::Value,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct HubMeasurementsIngestResponse {
    /// 新規に insert された件数。
    pub inserted: i64,
    /// unique index (tenant_id, device_id, seq, is_dev) 衝突でスキップされた件数 (同じ軸の再送重複)。
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
    /// 接続に使った hub の device_id (`HubMeasurementCreate::via_device_id`)。無ければ null。
    pub via_device_id: Option<String>,
    /// 端末を人に見せるときの ID (Refs ippoan/alc-cam#2)。hub の ID の行は、結び付いた機体の鍵の ID
    /// (`alarm:<fingerprint>`) に畳む。結び付きが無ければ `device_id` のまま。
    pub machine_id: String,
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
    /// 端末で絞る。機体の鍵の ID (`alarm:<fingerprint>`) を渡すと、その鍵に結び付いた hub の ID の
    /// 行も返す (Refs ippoan/alc-cam#2)。
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

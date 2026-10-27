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

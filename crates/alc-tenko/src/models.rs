//! tenko ドメインの models (alc-core から移設、Refs #513)。

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

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

// --- 点呼方法 / 判定の確認の方法 (Refs ippoan/alc-app#387) ---
//
// 値はここ 1 か所に置き、handler・CSV から同じ定数を使う。

/// 点呼方法 `IT点呼` (`tenko_sessions.tenko_method` / `tenko_records.tenko_method`)。
/// 通常点呼の流れの最後に運行管理者と通話する方法で、判定が付くまでは未完了として扱う
pub const TENKO_METHOD_IT: &str = "IT点呼";
/// 記録簿の CSV に出す点呼方法。対面で確定した IT点呼 の記録だけをこの値に読み替える
/// (DB には書かない — `tenko_method` の CHECK にこの値は無い)
pub const TENKO_METHOD_IN_PERSON: &str = "対面点呼";
/// 判定の確認の方法: 通話で確認した (`tenko_sessions.manager_judgment_method`)
pub const JUDGMENT_METHOD_IT: &str = "it";
/// 判定の確認の方法: 本人が来て対面で確認した
pub const JUDGMENT_METHOD_IN_PERSON: &str = "in_person";

// --- Tenko Session (点呼セッション) ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TenkoSession {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub employee_id: Uuid,
    pub schedule_id: Option<Uuid>,
    pub tenko_type: String,
    /// 自動点呼 / 通常点呼 / 遠隔点呼 (migration 141 で追加、144 で遠隔点呼を追加)。
    /// 既定は '自動点呼'。通常点呼は normal_tenko 経路が明示的に立てる。
    /// 遠隔点呼は escalate-remote (下記) でのみ立つ
    pub tenko_method: String,
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
    // 電子車検証 (migration 142、通常点呼だけが書く。Refs ippoan/alc-app-s3#110)
    /// 端末が読んだ管理番号
    #[serde(default)]
    pub carins_cert_no: Option<String>,
    /// 端末が読んだ車両 ID
    #[serde(default)]
    pub carins_vehicle_id: Option<String>,
    /// carins で照合できた車検期限
    #[serde(default)]
    pub carins_expires_on: Option<NaiveDate>,
    /// `cert_no` / `car_id` / `none`。NULL = 番号なし、または照合に失敗
    #[serde(default)]
    pub carins_matched_by: Option<String>,
    // 遠隔点呼への切り替え (migration 144、Refs ippoan/alc-app-s3#135)。
    // フィールド名 (= JSON キー) は `escalated_to_remote_at` — 画面側 (#c135-41) の
    // 管理者バッジがこの名前でフィールドの有無だけを見る (親の決定)。DB 列名も同じに
    // 揃えてある (rename attribute で JSON だけ合わせるより、名前を 1 つにする方が
    // 読み違いの余地がないため)
    #[serde(default)]
    pub escalated_to_remote_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub remote_escalation_reason: Option<String>,
    // 運行管理者による OK/NG 判定 (migration 148、Refs ippoan/alc-app#315)。
    // status は変えない — NG でも点呼は完了扱いのまま。NULL = 未判定
    #[serde(default)]
    pub manager_judgment: Option<String>,
    #[serde(default)]
    pub manager_judgment_reason: Option<String>,
    /// 判定した運行管理者 (employees.id への FK)。テナント管理者アカウントの
    /// user_id ではない — 「どの運行管理者が判断したか」を表す値が要るため
    #[serde(default)]
    pub manager_judgment_by: Option<Uuid>,
    /// 判定の確認の方法 (migration 157、Refs ippoan/alc-app#387)。
    /// `it` = 通話で確認 / `in_person` = 対面で確認 / NULL = 未確定、または IT点呼 でない
    #[serde(default)]
    pub manager_judgment_method: Option<String>,
    /// 運転者の本人確認の方法 (migration 159、Refs ippoan/alc-app#387)。
    /// `license` / `ic_card` / `remote_punch` / `nfc_card` / `manual`。
    /// 通常点呼の測定の保存 (PUT) だけが書く。NULL = 記録なし
    /// (この列より前の記録・方法を送らない端末・ほかの点呼の流れ)。認可の判断には使わない
    #[serde(default)]
    pub identity_method: Option<String>,
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
    /// 血圧必須判定に使う端末識別子 (Refs ippoan/alc-app#322)。判定結果 (bp_enabled) 自体は
    /// クライアントに送らせない — サーバが `devices.bp_enabled` を正本として引く
    #[serde(default)]
    pub device_id: Option<Uuid>,
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
    /// 点呼方法 (`IT点呼` など) で絞る (Refs ippoan/alc-app#387)
    pub tenko_method: Option<String>,
    /// `true` のとき、運行管理者の判定がまだ付いていないものだけに絞る。
    /// `false` と未指定は絞らない
    pub judgment_pending: Option<bool>,
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
    /// 通常点呼 / 遠隔点呼は執行者が付かないので NULL になりうる (migration 140)
    pub responsible_manager_name: Option<String>,
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
    // 運行管理者による OK/NG 判定 (Refs ippoan/alc-app#315)。tenko_records は
    // 挿入後 UPDATE 不可 (migration 015 のトリガー) なので、判定が記録時点より
    // 後に付くケースもカバーできるよう、CSV エクスポートの JOIN で都度
    // tenko_sessions から引く (= 固定値のスナップショットにしない)。他の
    // クエリ (list/get) はこの列を SELECT しないので常に None
    #[sqlx(default)]
    pub manager_judgment: Option<String>,
    #[sqlx(default)]
    pub manager_judgment_reason: Option<String>,
    /// 判定した運行管理者の氏名 (employees.name)。id のままでは帳票として読めないため、
    /// CSV 用の JOIN で名前まで引いておく
    #[sqlx(default)]
    pub manager_judgment_by_name: Option<String>,
    /// 判定の確認の方法 (`it` / `in_person`)。CSV 用の JOIN だけが引く。
    /// 対面で確定した IT点呼 の点呼方法を CSV で読み替えるのに使う (Refs ippoan/alc-app#387)。
    /// CSV の読み替えのためだけの欄なので JSON には出さない (記録の応答の形を変えない)
    #[sqlx(default)]
    #[serde(skip)]
    pub manager_judgment_method: Option<String>,
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

/// 運行管理者による点呼 OK/NG 判定 (Refs ippoan/alc-app#315)。
/// `judgment` は "ok" / "ng" のみ許す (ハンドラ側でバリデーション)。
/// `reason` は NG の理由。任意入力 (オーナー決定 3)。
/// `judged_by_employee_id` は判定した運行管理者の employee id — alc-app が顔認証で
/// 特定済みの employee を送る (テナント管理者アカウントの user_id ではない)。
/// ハンドラ側で同テナント・`deleted_at IS NULL`・`role` に `manager`/`admin` を
/// 含むことを検証する
#[derive(Debug, Deserialize)]
pub struct RecordManagerJudgment {
    pub judgment: String,
    #[serde(default)]
    pub reason: Option<String>,
    pub judged_by_employee_id: Uuid,
    /// 確認の方法 (`it` = 通話 / `in_person` = 対面)。IT点呼 のセッションを最初に確定する
    /// ときは必須、IT点呼 でないセッションには付けられない (ハンドラ側で検査。
    /// Refs ippoan/alc-app#387)。省略時は既存の値を保つ
    #[serde(default)]
    pub method: Option<String>,
}

/// 遠隔点呼への切り替え (Refs ippoan/alc-app-s3#135)。
/// 画面側 (#c135-41) と形が決まるまでの最小形: 理由の文字列のみ。
/// 切り替え時刻はサーバー側で NOW() を刻む
#[derive(Debug, Deserialize)]
pub struct EscalateToRemote {
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

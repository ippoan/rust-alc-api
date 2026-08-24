//! trouble ドメインの models (alc-core::models から移設、Refs #513 Phase B)。
//!
//! TS derive 付き (nuxt-trouble 向け型生成)。bindings は ts-rs が
//! CARGO_MANIFEST_DIR/bindings に出力し、CI の `crates/*/bindings/**` glob が拾う。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use ts_rs::TS;
use uuid::Uuid;

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
    pub person_is_external: bool,
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
    pub disciplinary_committee: String,
    pub road_service_cost: Option<String>,
    pub counterparty: String,
    pub counterparty_insurance: String,
    pub counterparty_vehicle: String,
    pub custom_fields: serde_json::Value,
    pub due_date: Option<DateTime<Utc>>,
    pub overdue_notified_at: Option<DateTime<Utc>>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Default, Deserialize, TS)]
#[ts(export)]
pub struct CreateTroubleTicket {
    pub category: String,
    pub title: Option<String>,
    #[serde(
        default,
        deserialize_with = "alc_core::serde_helpers::empty_string_as_none_datetime"
    )]
    pub occurred_at: Option<DateTime<Utc>>,
    pub occurred_date: Option<chrono::NaiveDate>,
    pub company_name: Option<String>,
    pub office_name: Option<String>,
    pub department: Option<String>,
    pub person_name: Option<String>,
    #[serde(
        default,
        deserialize_with = "alc_core::serde_helpers::empty_string_as_none_uuid"
    )]
    pub person_id: Option<Uuid>,
    #[serde(default)]
    pub person_is_external: Option<bool>,
    pub registration_number: Option<String>,
    pub location: Option<String>,
    pub description: Option<String>,
    #[serde(
        default,
        deserialize_with = "alc_core::serde_helpers::empty_string_as_none_uuid"
    )]
    pub assigned_to: Option<Uuid>,
    pub damage_amount: Option<f64>,
    pub compensation_amount: Option<f64>,
    pub road_service_cost: Option<f64>,
    pub counterparty: Option<String>,
    pub counterparty_insurance: Option<String>,
    pub custom_fields: Option<serde_json::Value>,
    #[serde(
        default,
        deserialize_with = "alc_core::serde_helpers::empty_string_as_none_datetime"
    )]
    pub due_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct UpdateTroubleTicket {
    pub category: Option<String>,
    pub title: Option<String>,
    #[serde(
        default,
        deserialize_with = "alc_core::serde_helpers::empty_string_as_none_datetime"
    )]
    pub occurred_at: Option<DateTime<Utc>>,
    pub occurred_date: Option<chrono::NaiveDate>,
    pub company_name: Option<String>,
    pub office_name: Option<String>,
    pub department: Option<String>,
    pub person_name: Option<String>,
    #[serde(
        default,
        deserialize_with = "alc_core::serde_helpers::empty_string_as_none_uuid"
    )]
    pub person_id: Option<Uuid>,
    #[serde(default)]
    pub person_is_external: Option<bool>,
    #[serde(
        default,
        deserialize_with = "alc_core::serde_helpers::empty_string_as_none_option_string"
    )]
    pub registration_number: Option<Option<String>>,
    pub location: Option<String>,
    pub description: Option<String>,
    #[serde(
        default,
        deserialize_with = "alc_core::serde_helpers::empty_string_as_none_uuid"
    )]
    pub assigned_to: Option<Uuid>,
    pub progress_notes: Option<String>,
    pub allowance: Option<String>,
    pub damage_amount: Option<f64>,
    pub compensation_amount: Option<f64>,
    pub confirmation_notice: Option<String>,
    pub disciplinary_content: Option<String>,
    pub disciplinary_action: Option<String>,
    pub disciplinary_committee: Option<String>,
    pub road_service_cost: Option<f64>,
    pub counterparty: Option<String>,
    pub counterparty_insurance: Option<String>,
    pub counterparty_vehicle: Option<String>,
    pub custom_fields: Option<serde_json::Value>,
    #[serde(
        default,
        deserialize_with = "alc_core::serde_helpers::empty_string_as_none_datetime"
    )]
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
    /// ソート対象カラム。whitelist ("occurred" | "ticket_no") 以外・未指定は
    /// 既定の ticket_no 降順 (Refs ippoan/nuxt-trouble#225)。
    pub sort_by: Option<String>,
    pub sort_desc: Option<bool>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct TroubleTicketsResponse {
    pub tickets: Vec<TroubleTicket>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

/// トラブルチケット入力フォームの1フィールド分の表示設定 (表示/非表示・幅・並び順・
/// ラベル付け替え)。フィールドの type 等それ以外の静的メタデータはフロントエンド側が
/// 持ち、ここではテナントが上書きした値だけを保持する。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TroubleFieldLayoutEntry {
    pub key: String,
    pub visible: bool,
    pub width: String,
    pub sort_order: i32,
    /// テナントが付け替えた表示ラベル。未設定 (None) ならフロントエンドの
    /// デフォルトラベルを使う。
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TroubleFieldLayout {
    pub settings: Vec<TroubleFieldLayoutEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export)]
pub struct TroubleFile {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub ticket_id: Uuid,
    pub task_id: Option<Uuid>,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub storage_key: String,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
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

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export)]
pub struct TroubleTaskStatus {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub key: String,
    pub name: String,
    pub color: String,
    pub sort_order: i32,
    pub is_done: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateTroubleTaskStatus {
    pub key: Option<String>,
    pub name: String,
    pub color: Option<String>,
    pub sort_order: Option<i32>,
    pub is_done: Option<bool>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct UpdateTroubleTaskStatus {
    pub name: Option<String>,
    pub color: Option<String>,
    pub sort_order: Option<i32>,
    pub is_done: Option<bool>,
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
    pub next_action_detail: String,
    pub next_action_by: Option<String>,
    pub next_action_due: Option<DateTime<Utc>>,
    pub occurred_at: Option<DateTime<Utc>>,
    /// 印刷時、この行の直前で改ページするか (ユーザーが任意の位置に手動指定)。
    pub print_page_break_before: bool,
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
    #[serde(
        default,
        deserialize_with = "alc_core::serde_helpers::empty_string_as_none_uuid"
    )]
    pub assigned_to: Option<Uuid>,
    #[serde(
        default,
        deserialize_with = "alc_core::serde_helpers::empty_string_as_none_datetime"
    )]
    pub due_date: Option<DateTime<Utc>>,
    #[serde(default)]
    pub sort_order: Option<i32>,
    #[serde(default)]
    pub next_action: Option<String>,
    #[serde(default)]
    pub next_action_detail: Option<String>,
    #[serde(default)]
    pub next_action_by: Option<String>,
    #[serde(
        default,
        deserialize_with = "alc_core::serde_helpers::empty_string_as_none_datetime"
    )]
    pub next_action_due: Option<DateTime<Utc>>,
    #[serde(
        default,
        deserialize_with = "alc_core::serde_helpers::empty_string_as_none_datetime"
    )]
    pub occurred_at: Option<DateTime<Utc>>,
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
    #[serde(
        default,
        deserialize_with = "alc_core::serde_helpers::empty_string_as_none_option_uuid"
    )]
    pub assigned_to: Option<Option<Uuid>>,
    #[serde(
        default,
        deserialize_with = "alc_core::serde_helpers::empty_string_as_none_option_datetime"
    )]
    pub due_date: Option<Option<DateTime<Utc>>>,
    #[serde(
        default,
        deserialize_with = "alc_core::serde_helpers::empty_string_as_none_option_datetime"
    )]
    pub completed_at: Option<Option<DateTime<Utc>>>,
    #[serde(default)]
    pub sort_order: Option<i32>,
    #[serde(default)]
    pub next_action: Option<String>,
    #[serde(default)]
    pub next_action_detail: Option<String>,
    #[serde(
        default,
        deserialize_with = "alc_core::serde_helpers::empty_string_as_none_option_string"
    )]
    pub next_action_by: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "alc_core::serde_helpers::empty_string_as_none_option_datetime"
    )]
    pub next_action_due: Option<Option<DateTime<Utc>>>,
    #[serde(
        default,
        deserialize_with = "alc_core::serde_helpers::empty_string_as_none_option_datetime"
    )]
    pub occurred_at: Option<Option<DateTime<Utc>>>,
    #[serde(default)]
    pub print_page_break_before: Option<bool>,
}

/// 経過記録 (trouble_tasks) の並び替え要求。`task_ids` に並べたい順で id を
/// 全件渡し、サーバが 0 起点で `sort_order` を採番し直す。隣接行の交換方式は
/// 全行 sort_order=0 の既存データで無変化になるため採らない (Refs
/// ippoan/nuxt-trouble#240)。
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ReorderTroubleTasks {
    pub task_ids: Vec<Uuid>,
}

#[cfg(test)]
mod update_trouble_task_tests {
    use super::UpdateTroubleTask;

    #[test]
    fn next_action_by_absent_does_not_update() {
        let t: UpdateTroubleTask = serde_json::from_str(r#"{}"#).unwrap();
        assert!(t.next_action_by.is_none());
    }

    #[test]
    fn next_action_by_null_clears() {
        let t: UpdateTroubleTask = serde_json::from_str(r#"{"next_action_by": null}"#).unwrap();
        assert_eq!(t.next_action_by, Some(None));
    }

    #[test]
    fn next_action_by_value_updates() {
        let t: UpdateTroubleTask =
            serde_json::from_str(r#"{"next_action_by": "青井 健"}"#).unwrap();
        assert_eq!(t.next_action_by, Some(Some("青井 健".to_string())));
    }
}

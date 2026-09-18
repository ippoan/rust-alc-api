use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use ts_rs::TS;
use uuid::Uuid;

/// 車両マスタ 1 行 (`maintenance_vehicles`)。車両 identity の正本 — 電子車検証
/// (`car_id`) は従属する任意のリンクで、NULL のままでも登録できる (Refs #651)。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export)]
pub struct MaintenanceVehicle {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub registration_number: String,
    pub display_name: Option<String>,
    pub car_id: Option<String>,
    pub carins_linked_at: Option<DateTime<Utc>>,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

/// `POST /api/maintenance/vehicles` の body。`registration_number` だけで
/// 作成できる (`car_id` は任意 — 車検証が無くても登録できる、Refs #651)。
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct CreateMaintenanceVehicle {
    pub registration_number: String,
    pub display_name: Option<String>,
    pub car_id: Option<String>,
    pub note: Option<String>,
}

/// `PUT /api/maintenance/vehicles/{id}` の body。`None` のフィールドは変更しない
/// (COALESCE 意味論。`alc-misc::employees` の免許証更新と同じ作法)。
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct UpdateMaintenanceVehicle {
    pub registration_number: Option<String>,
    pub display_name: Option<String>,
    pub note: Option<String>,
}

/// `GET /api/maintenance/vehicles` のクエリパラメータ。
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct VehicleListFilter {
    /// 登録番号・display_name の部分一致
    pub q: Option<String>,
    /// carins (car_id) 紐づけ済みかどうか
    pub linked: Option<bool>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct VehicleListResponse {
    pub items: Vec<MaintenanceVehicle>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

/// `PUT /api/maintenance/vehicles/{id}/carins` の body。どちらか一方でよい —
/// `alc-core::repository::car_inspections::lookup_expiry` は管理番号 (`cert_no`) と
/// 車両 ID (`car_id`) の OR で一致する行のうち期限の新しい 1 行を返す。
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct LinkCarinsRequest {
    pub cert_no: Option<String>,
    pub car_id: Option<String>,
}

/// `GET /api/maintenance/vehicles/{id}/carins-candidates` の候補 1 件。
/// 所有者・住所・車台番号は返さない (`CarinsLookup` と同じ最小方針)。
#[derive(Debug, Clone, Serialize, FromRow, TS)]
#[ts(export)]
pub struct CarinsCandidate {
    pub car_id: String,
    pub cert_no: String,
    /// 電子車検証側の登録番号相当 (`EntryNoCarNo` / `CarNo` のうち非空の方)
    pub car_no: String,
}

/// `GET /api/maintenance/vehicles/carins-import-candidates` の候補 1 件。
/// **まだ `maintenance_vehicles` に取り込まれていない**電子車検証を返す
/// (`CarinsCandidate` の「この車両に一致する候補」の反転、Refs #662)。
/// 所有者・住所・車台番号は返さない (`CarinsCandidate` と同じ最小方針)。
#[derive(Debug, Clone, Serialize, FromRow, TS)]
#[ts(export)]
pub struct CarinsImportCandidate {
    pub car_id: String,
    pub cert_no: String,
    /// 電子車検証側の登録番号相当 (`EntryNoCarNo` / `CarNo` のうち非空の方)
    pub car_no: String,
    /// 正規化した登録番号で一致した既存車両。`None` = 未登録 (取り込みで新規作成)、
    /// `Some` = 既存行に `car_id` を紐づけるだけで済む。
    pub existing_vehicle_id: Option<Uuid>,
}

/// `POST /api/maintenance/vehicles/carins-import` の body。
/// `carins-import-candidates` が返した `car_id` のうち、利用者が選んだものを渡す。
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct CarinsImportRequest {
    pub car_ids: Vec<String>,
}

/// `POST /api/maintenance/vehicles/carins-import` の応答。**件数のみ** — 行は返さない
/// (取り込み後の一覧は `GET /api/maintenance/vehicles` で取り直す、Refs #662)。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CarinsImportResult {
    /// 登録番号一致の既存車両が無く、新規作成した件数
    pub created: i64,
    /// 既存車両に `car_id` を紐づけた件数
    pub linked: i64,
    /// 既に紐づけ済み / 一致先が別の車検証に紐づけ済み / このテナントに無い `car_id`
    pub skipped: i64,
}

/// 整備カテゴリ 1 行 (`maintenance_categories`)。`alc-trouble` の
/// `TroubleCategory` と同じ形の generic master (`alc_core::master_data`) で
/// CRUD する (Refs #651)。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export)]
pub struct MaintenanceCategory {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

/// `POST /api/maintenance/categories` の body。
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateMaintenanceCategory {
    pub name: String,
    pub sort_order: Option<i32>,
}

/// 整備記録 1 行 (`maintenance_records`)。国交省の遠隔点呼要件 2-四-ト
/// 「運行に使用する事業用自動車の整備状況」を満たすための、車両本体の整備記録
/// (定期点検・修理・部品交換等、Refs #651)。`cost` は `NUMERIC(12,2)` を
/// `::text` キャストして文字列で保持する (`alc-trouble::TroubleTicket` の
/// `damage_amount` と同じ作法 — `f64` へ丸めない)。
#[derive(Debug, Clone, Serialize, FromRow, TS)]
#[ts(export)]
pub struct MaintenanceRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub vehicle_id: Uuid,
    pub category_id: Uuid,
    /// 整備実施日
    pub performed_on: chrono::NaiveDate,
    /// 走行距離 (km)
    pub odometer_km: Option<i32>,
    /// 整備工場
    pub vendor: Option<String>,
    pub description: Option<String>,
    pub cost: Option<String>,
    /// 次回期限
    pub next_due_on: Option<chrono::NaiveDate>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

/// `POST /api/maintenance/records` の body。`vehicle_id` / `category_id` は
/// **同じテナントのものであること**を handler 側で明示確認する — DB の FK は
/// 行の存在しか保証せずテナントは保証しないため (Refs #651)。
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct CreateMaintenanceRecord {
    pub vehicle_id: Uuid,
    pub category_id: Uuid,
    pub performed_on: chrono::NaiveDate,
    pub odometer_km: Option<i32>,
    pub vendor: Option<String>,
    pub description: Option<String>,
    pub cost: Option<f64>,
    pub next_due_on: Option<chrono::NaiveDate>,
}

/// `PUT /api/maintenance/records/{id}` の body。`None` のフィールドは変更しない
/// (COALESCE 意味論。`UpdateMaintenanceVehicle` と同じ作法)。`vehicle_id` /
/// `category_id` を変更する場合も同じテナント確認を handler 側で行う。
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct UpdateMaintenanceRecord {
    pub vehicle_id: Option<Uuid>,
    pub category_id: Option<Uuid>,
    pub performed_on: Option<chrono::NaiveDate>,
    pub odometer_km: Option<i32>,
    pub vendor: Option<String>,
    pub description: Option<String>,
    pub cost: Option<f64>,
    pub next_due_on: Option<chrono::NaiveDate>,
}

/// `GET /api/maintenance/records` のクエリパラメータ。
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct MaintenanceRecordListFilter {
    pub vehicle_id: Option<Uuid>,
    pub category_id: Option<Uuid>,
    /// `performed_on` に対する範囲検索 (以上)
    pub date_from: Option<chrono::NaiveDate>,
    /// `performed_on` に対する範囲検索 (以下)
    pub date_to: Option<chrono::NaiveDate>,
    /// `description` / `vendor` の部分一致
    pub q: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct MaintenanceRecordsResponse {
    pub records: Vec<MaintenanceRecord>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

/// 整備記録の添付ファイル (`maintenance_files`)。`crates/alc-trouble/src/models.rs`
/// の `TroubleFile` と同じ列構成 (migrations/147:72 のコメント通り、Refs #651)。
/// サムネイルは生成しない — `content_type` をそのまま保存・返却する
/// (フロントが原寸を縮小表示する)。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, TS)]
#[ts(export)]
pub struct MaintenanceFile {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub record_id: Uuid,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub storage_key: String,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

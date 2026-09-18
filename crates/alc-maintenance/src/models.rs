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

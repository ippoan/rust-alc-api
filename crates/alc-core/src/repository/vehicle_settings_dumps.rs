use async_trait::async_trait;
use uuid::Uuid;

use crate::models::VehicleSettingsDump;

/// `vehicle_settings_dumps` テーブルの Repository trait。
///
/// フロント (nuxt-dtako-admin) が R2 への PUT 成功後に `register` を呼んで
/// dump メタデータを INSERT する。読み出しは「車輛別履歴」「テナント集計」の 2 パターン。
#[async_trait]
pub trait VehicleSettingsDumpsRepository: Send + Sync {
    /// dump の登録 (冪等: 同じ (tenant_id, vehicle_cd, dump_dir) は ON CONFLICT で UPDATE)。
    async fn register(
        &self,
        tenant_id: Uuid,
        input: VehicleSettingsDumpInput,
    ) -> Result<VehicleSettingsDump, sqlx::Error>;

    /// 指定 vehicle_cd の dump を uploaded_at DESC で返す。
    async fn list_by_vehicle_cd(
        &self,
        tenant_id: Uuid,
        vehicle_cd: &str,
    ) -> Result<Vec<VehicleSettingsDump>, sqlx::Error>;

    /// テナントの全車輛分集計 (vehicle_cd, count, latest_uploaded_at)。
    async fn summary_by_vehicle(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<VehicleSettingsDumpSummary>, sqlx::Error>;

    /// テナントに dump が存在する vehicle_cd 集合 (未確認車輛抽出用)。
    async fn confirmed_vehicle_cds(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<String>, sqlx::Error>;
}

#[derive(Debug, Clone)]
pub struct VehicleSettingsDumpInput {
    pub vehicle_cd: String,
    pub dump_dir: String,
    pub machine_id: Option<String>,
    pub firm_main_app: Option<String>,
    pub r2_json_key: String,
    pub r2_cfg_key: String,
    pub uploaded_by: Option<Uuid>,
}

// FromRow を derive しておくことで query_as が使える。
// trait 定義と一緒に alc-core 側に記述して、orphan rule 違反を避ける
// (同じコードによって PgVehicleSettingsDumpsRepository が summary_by_vehicle を query_as で使える)。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct VehicleSettingsDumpSummary {
    pub vehicle_cd: String,
    pub count: i64,
    pub latest_uploaded_at: chrono::DateTime<chrono::Utc>,
}

//! `/api/dtako/vehicle-settings-dumps` エンドポイント。
//!
//! フロント (nuxt-dtako-admin) が R2 へ PUT した後にここを叩いて
//! dump メタデータを DB に登録する。読み出しは 「車輛別履歴」「テナント合計」。
//!
//! ohishi-exp/nuxt-dtako-admin#39 でフロントが R2 list を直接叩いていた部分を
//! 本 endpoint で置き換えると、list の cursor pagination をスキップして
//! 1 query で済むようになる。

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use alc_core::auth_middleware::TenantId;
use alc_core::models::VehicleSettingsDump;
use alc_core::repository::vehicle_settings_dumps::{
    VehicleSettingsDumpInput, VehicleSettingsDumpSummary,
};

use crate::DtakoState;

pub fn tenant_router<S>() -> Router<S>
where
    DtakoState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    // axum v0.8+ では `:capture` でなく `{capture}` を使う
    Router::new()
        .route("/vehicle-settings-dumps", post(register_dump))
        .route("/vehicle-settings-dumps/summary", get(list_summary))
        .route("/vehicle-settings-dumps/{vehicle_cd}", get(list_by_vehicle))
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub vehicle_cd: String,
    pub dump_dir: String,
    pub machine_id: Option<String>,
    pub firm_main_app: Option<String>,
    pub r2_json_key: String,
    pub r2_cfg_key: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    #[serde(flatten)]
    pub dump: VehicleSettingsDump,
}

async fn register_dump(
    State(state): State<DtakoState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, StatusCode> {
    let tenant_id = tenant.0 .0;

    // 入力バリデーション: empty / 制限超え
    if body.vehicle_cd.is_empty() || body.vehicle_cd.len() > 32 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if body.dump_dir.is_empty() || body.dump_dir.len() > 64 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if !body.r2_json_key.starts_with("vehicle-settings/")
        || !body.r2_cfg_key.starts_with("vehicle-settings/")
    {
        return Err(StatusCode::BAD_REQUEST);
    }

    // uploaded_by は現時点では記録しない (AuthUser の user_id は String だがカラムは UUID
    // なので、パース処理をそろえるまでは保留)。Phase 4 で考える。
    let input = VehicleSettingsDumpInput {
        vehicle_cd: body.vehicle_cd,
        dump_dir: body.dump_dir,
        machine_id: body.machine_id,
        firm_main_app: body.firm_main_app,
        r2_json_key: body.r2_json_key,
        r2_cfg_key: body.r2_cfg_key,
        uploaded_by: None,
    };

    let dump = state
        .vehicle_settings_dumps
        .register(tenant_id, input)
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "failed to register vehicle settings dump");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(RegisterResponse { dump }))
}

#[derive(Debug, Deserialize)]
pub struct ListByVehicleQuery {
    // 予約 (将来 limit / cursor を取りたいときを見越して struct 化)
}

async fn list_by_vehicle(
    State(state): State<DtakoState>,
    tenant: axum::Extension<TenantId>,
    Path(vehicle_cd): Path<String>,
    Query(_q): Query<ListByVehicleQuery>,
) -> Result<Json<Vec<VehicleSettingsDump>>, StatusCode> {
    let tenant_id = tenant.0 .0;
    if vehicle_cd.is_empty() || vehicle_cd.len() > 32 {
        return Err(StatusCode::BAD_REQUEST);
    }

    let dumps = state
        .vehicle_settings_dumps
        .list_by_vehicle_cd(tenant_id, &vehicle_cd)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(dumps))
}

async fn list_summary(
    State(state): State<DtakoState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<VehicleSettingsDumpSummary>>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let summary = state
        .vehicle_settings_dumps
        .summary_by_vehicle(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(summary))
}

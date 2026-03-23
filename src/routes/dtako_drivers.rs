use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use serde::Serialize;
use uuid::Uuid;

use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::TenantId;
use crate::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new().route("/drivers", get(list_drivers))
}

/// daiun-salary 互換の Driver レスポンス
#[derive(Debug, Serialize, sqlx::FromRow)]
struct Driver {
    id: Uuid,
    tenant_id: Uuid,
    driver_cd: Option<String>,
    #[sqlx(rename = "name")]
    #[serde(rename = "driver_name")]
    driver_name: String,
}

async fn list_drivers(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<Driver>>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let mut conn = state.pool.acquire().await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string()).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let drivers = sqlx::query_as::<_, Driver>(
        "SELECT id, tenant_id, driver_cd, name FROM alc_api.employees WHERE driver_cd IS NOT NULL AND deleted_at IS NULL ORDER BY driver_cd",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(drivers))
}

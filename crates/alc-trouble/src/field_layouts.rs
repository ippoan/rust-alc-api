use axum::{extract::State, http::StatusCode, routing::get, Json, Router};

use crate::TroubleState;
use alc_core::auth_middleware::TenantId;
use alc_core::models::TroubleFieldLayout;

pub fn tenant_router<S>() -> Router<S>
where
    TroubleState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new().route(
        "/trouble/field-layout",
        get(get_field_layout).put(update_field_layout),
    )
}

async fn get_field_layout(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<TroubleFieldLayout>, StatusCode> {
    let layout = state
        .trouble_field_layouts
        .get(tenant.0 .0)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(layout))
}

async fn update_field_layout(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<TroubleFieldLayout>,
) -> Result<Json<TroubleFieldLayout>, StatusCode> {
    let layout = state
        .trouble_field_layouts
        .upsert(tenant.0 .0, &body)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(layout))
}

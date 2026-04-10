use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};
use uuid::Uuid;

use crate::TroubleState;
use alc_core::auth_middleware::TenantId;
use alc_core::models::{CreateTroubleOffice, TroubleOffice};

pub fn tenant_router<S>() -> Router<S>
where
    TroubleState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/trouble/offices", get(list_offices).post(create_office))
        .route("/trouble/offices/{id}", delete(delete_office))
}

async fn list_offices(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<TroubleOffice>>, StatusCode> {
    let offices = state
        .trouble_offices
        .list(tenant.0 .0)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(offices))
}

async fn create_office(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateTroubleOffice>,
) -> Result<(StatusCode, Json<TroubleOffice>), StatusCode> {
    if body.name.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let office = state
        .trouble_offices
        .create(tenant.0 .0, &body)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e {
                if db_err.constraint().is_some() {
                    return StatusCode::CONFLICT;
                }
            }
            tracing::error!("create_office error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok((StatusCode::CREATED, Json(office)))
}

async fn delete_office(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let deleted = state
        .trouble_offices
        .delete(tenant.0 .0, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

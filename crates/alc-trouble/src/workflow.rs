use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::TroubleState;
use alc_core::auth_middleware::TenantId;
use alc_core::models::{
    CreateWorkflowState, CreateWorkflowTransition, TroubleStatusHistory, TroubleWorkflowState,
    TroubleWorkflowTransition,
};

pub fn tenant_router<S>() -> Router<S>
where
    TroubleState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/trouble/workflow/states",
            get(list_states).post(create_state),
        )
        .route("/trouble/workflow/states/{id}", delete(delete_state))
        .route(
            "/trouble/workflow/transitions",
            get(list_transitions).post(create_transition),
        )
        .route(
            "/trouble/workflow/transitions/{id}",
            delete(delete_transition),
        )
        .route("/trouble/workflow/setup", post(setup_defaults))
        .route("/trouble/tickets/{ticket_id}/history", get(list_history))
}

async fn list_states(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<TroubleWorkflowState>>, StatusCode> {
    let states = state
        .trouble_workflow
        .list_states(tenant.0 .0)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(states))
}

async fn create_state(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateWorkflowState>,
) -> Result<(StatusCode, Json<TroubleWorkflowState>), StatusCode> {
    let ws = state
        .trouble_workflow
        .create_state(tenant.0 .0, &body)
        .await
        .map_err(|e| {
            tracing::error!("create_state error: {e}");
            if e.to_string().contains("duplicate") {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;
    Ok((StatusCode::CREATED, Json(ws)))
}

async fn delete_state(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let deleted = state
        .trouble_workflow
        .delete_state(tenant.0 .0, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn list_transitions(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<TroubleWorkflowTransition>>, StatusCode> {
    let transitions = state
        .trouble_workflow
        .list_transitions(tenant.0 .0)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(transitions))
}

async fn create_transition(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateWorkflowTransition>,
) -> Result<(StatusCode, Json<TroubleWorkflowTransition>), StatusCode> {
    let tr = state
        .trouble_workflow
        .create_transition(tenant.0 .0, &body)
        .await
        .map_err(|e| {
            tracing::error!("create_transition error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok((StatusCode::CREATED, Json(tr)))
}

async fn delete_transition(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let deleted = state
        .trouble_workflow
        .delete_transition(tenant.0 .0, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn setup_defaults(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<TroubleWorkflowState>>, StatusCode> {
    let states = state
        .trouble_workflow
        .setup_defaults(tenant.0 .0)
        .await
        .map_err(|e| {
            tracing::error!("setup_defaults error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(states))
}

async fn list_history(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<Vec<TroubleStatusHistory>>, StatusCode> {
    let history = state
        .trouble_workflow
        .list_history(tenant.0 .0, ticket_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(history))
}

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, post},
    Json, Router,
};
use uuid::Uuid;

use crate::TroubleState;
use alc_core::auth_middleware::TenantId;
use alc_core::models::{CreateTroubleComment, TroubleComment};

pub fn tenant_router<S>() -> Router<S>
where
    TroubleState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/trouble/tickets/{ticket_id}/comments",
            post(create_comment).get(list_comments),
        )
        .route("/trouble/comments/{id}", delete(delete_comment))
}

async fn create_comment(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(ticket_id): Path<Uuid>,
    Json(body): Json<CreateTroubleComment>,
) -> Result<(StatusCode, Json<TroubleComment>), StatusCode> {
    if body.body.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let tenant_id = tenant.0 .0;

    let comment = state
        .trouble_comments
        .create(tenant_id, ticket_id, None, &body)
        .await
        .map_err(|e| {
            tracing::error!("create_comment error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // LINE WORKS Bot 通知
    if let Some(notifier) = &state.notifier {
        if let Ok(Some(pref)) = state
            .trouble_notification_prefs
            .find_enabled(tenant_id, "trouble_comment_added", "lineworks")
            .await
        {
            let msg = format!("コメント追加: {}", body.body);
            notifier
                .notify(
                    tenant_id,
                    "trouble_comment_added",
                    &msg,
                    &pref.lineworks_user_ids,
                )
                .await;
        }
    }

    Ok((StatusCode::CREATED, Json(comment)))
}

async fn list_comments(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<Vec<TroubleComment>>, StatusCode> {
    let comments = state
        .trouble_comments
        .list_by_ticket(tenant.0 .0, ticket_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(comments))
}

async fn delete_comment(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let deleted = state
        .trouble_comments
        .delete(tenant.0 .0, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::TroubleState;
use alc_core::auth_middleware::TenantId;
use alc_core::models::{CreateTroubleSchedule, TroubleSchedule};

pub fn tenant_router<S>() -> Router<S>
where
    TroubleState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/trouble/schedules", post(create_schedule))
        .route(
            "/trouble/tickets/{ticket_id}/schedules",
            get(list_schedules),
        )
        .route(
            "/trouble/schedules/{id}",
            axum::routing::delete(cancel_schedule),
        )
}

/// Cloud Tasks から呼ばれるルート (認証は別途)
pub fn fire_router<S>() -> Router<S>
where
    TroubleState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new().route("/trouble/schedules/{id}/fire", post(fire_schedule))
}

async fn create_schedule(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateTroubleSchedule>,
) -> Result<(StatusCode, Json<TroubleSchedule>), StatusCode> {
    let tenant_id = tenant.0 .0;

    // 30日先までの制限
    let max_future = chrono::Utc::now() + chrono::Duration::days(30);
    if body.scheduled_at > max_future {
        return Err(StatusCode::BAD_REQUEST);
    }
    if body.scheduled_at <= chrono::Utc::now() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if body.message.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if body.lineworks_user_ids.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let schedule = state
        .trouble_schedules
        .create(tenant_id, &body, None)
        .await
        .map_err(|e| {
            tracing::error!("create_schedule DB error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Cloud Tasks にタスク登録
    if let Some(ct) = &state.cloud_tasks {
        match ct.create_task(schedule.id, schedule.scheduled_at).await {
            Ok(task_name) => {
                let _ = state
                    .trouble_schedules
                    .set_cloud_task_name(tenant_id, schedule.id, &task_name)
                    .await;
            }
            Err(e) => {
                tracing::error!("Cloud Tasks create error: {e}");
                // タスク登録失敗してもスケジュール自体は保存済み
            }
        }
    }

    Ok((StatusCode::CREATED, Json(schedule)))
}

async fn list_schedules(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<Vec<TroubleSchedule>>, StatusCode> {
    let schedules = state
        .trouble_schedules
        .list_by_ticket(tenant.0 .0, ticket_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(schedules))
}

async fn cancel_schedule(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let tenant_id = tenant.0 .0;

    // まず取得してcloud_task_nameを確認
    let schedule = state
        .trouble_schedules
        .get(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if schedule.status != "pending" {
        return Err(StatusCode::CONFLICT);
    }

    // Cloud Tasks からタスク削除
    if let (Some(ct), Some(task_name)) = (&state.cloud_tasks, &schedule.cloud_task_name) {
        if let Err(e) = ct.delete_task(task_name).await {
            tracing::error!("Cloud Tasks delete error: {e}");
        }
    }

    let cancelled = state
        .trouble_schedules
        .update_status(tenant_id, id, "cancelled")
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if cancelled {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// Cloud Tasks から呼ばれる fire エンドポイント
async fn fire_schedule(
    State(state): State<TroubleState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    // RLS バイパスで取得
    let schedule = state
        .trouble_schedules
        .get_for_fire(id)
        .await
        .map_err(|e| {
            tracing::error!("fire_schedule get error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if schedule.status != "pending" {
        return Ok(StatusCode::OK);
    }

    // 通知送信
    if let Some(notifier) = &state.notifier {
        notifier
            .notify(
                schedule.tenant_id,
                "trouble_schedule",
                &schedule.message,
                &schedule.lineworks_user_ids,
            )
            .await;
    }

    // 送信済みマーク
    let _ = state.trouble_schedules.mark_sent(id).await;

    Ok(StatusCode::OK)
}

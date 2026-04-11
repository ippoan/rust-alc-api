use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::TroubleState;
use alc_core::auth_middleware::TenantId;
use alc_core::models::{
    CreateTroubleTicket, TransitionRequest, TroubleTicket, TroubleTicketFilter,
    TroubleTicketsResponse, UpdateTroubleTicket,
};

pub fn tenant_router<S>() -> Router<S>
where
    TroubleState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/trouble/tickets", post(create_ticket).get(list_tickets))
        .route("/trouble/tickets/csv", get(export_csv))
        .route(
            "/trouble/tickets/{id}",
            get(get_ticket).put(update_ticket).delete(delete_ticket),
        )
        .route("/trouble/tickets/{id}/transition", post(transition_ticket))
}

async fn create_ticket(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateTroubleTicket>,
) -> Result<(StatusCode, Json<TroubleTicket>), StatusCode> {
    let tenant_id = tenant.0 .0;

    let db_categories = state
        .trouble_categories
        .list(tenant_id)
        .await
        .unwrap_or_default();
    let valid = if db_categories.is_empty() {
        crate::DEFAULT_CATEGORIES.contains(&body.category.as_str())
    } else {
        db_categories.iter().any(|c| c.name == body.category)
    };
    if !valid {
        return Err(StatusCode::BAD_REQUEST);
    }

    // 初期ステータスを取得
    let initial_status = state
        .trouble_workflow
        .get_initial_state(tenant_id)
        .await
        .map_err(|e| {
            tracing::error!("get_initial_state error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let ticket = state
        .trouble_tickets
        .create(tenant_id, &body, None, initial_status.map(|s| s.id))
        .await
        .map_err(|e| {
            tracing::error!("create_ticket DB error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // ステータス履歴を記録
    if let Some(status_id) = ticket.status_id {
        let _ = state
            .trouble_workflow
            .record_history(tenant_id, ticket.id, None, status_id, None, None)
            .await;
    }

    // webhook通知
    if let Some(webhook) = &state.webhook {
        let payload = serde_json::json!({
            "event": "trouble_created",
            "timestamp": chrono::Utc::now(),
            "tenant_id": tenant_id,
            "data": {
                "ticket_id": ticket.id,
                "ticket_no": ticket.ticket_no,
                "category": ticket.category,
                "person_name": ticket.person_name,
                "description": ticket.description,
            }
        });
        webhook
            .fire_event(tenant_id, "trouble_created", payload)
            .await;
    }

    // LINE WORKS Bot 通知
    if let Some(notifier) = &state.notifier {
        if let Ok(Some(pref)) = state
            .trouble_notification_prefs
            .find_enabled(tenant_id, "trouble_created", "lineworks")
            .await
        {
            let msg = format!(
                "トラブル登録: #{} {}\nカテゴリ: {}\n担当者: {}",
                ticket.ticket_no, ticket.category, ticket.person_name, ticket.description,
            );
            notifier
                .notify(tenant_id, "trouble_created", &msg, &pref.lineworks_user_ids)
                .await;
        }
    }

    Ok((StatusCode::CREATED, Json(ticket)))
}

async fn list_tickets(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<TroubleTicketFilter>,
) -> Result<Json<TroubleTicketsResponse>, StatusCode> {
    let response = state
        .trouble_tickets
        .list(tenant.0 .0, &filter)
        .await
        .map_err(|e| {
            tracing::error!("list_tickets error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(response))
}

async fn get_ticket(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<TroubleTicket>, StatusCode> {
    let ticket = state
        .trouble_tickets
        .get(tenant.0 .0, id)
        .await
        .map_err(|e| {
            tracing::error!("get_ticket error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(ticket))
}

async fn update_ticket(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateTroubleTicket>,
) -> Result<Json<TroubleTicket>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let has_assigned_to = body.assigned_to.is_some();

    let ticket = state
        .trouble_tickets
        .update(tenant_id, id, &body)
        .await
        .map_err(|e| {
            tracing::error!("update_ticket error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    // assigned_to が設定された場合に LINE WORKS Bot 通知
    if has_assigned_to {
        if let Some(notifier) = &state.notifier {
            if let Ok(Some(pref)) = state
                .trouble_notification_prefs
                .find_enabled(tenant_id, "trouble_assigned", "lineworks")
                .await
            {
                let msg = format!("担当者アサイン: #{} {}", ticket.ticket_no, ticket.category,);
                notifier
                    .notify(
                        tenant_id,
                        "trouble_assigned",
                        &msg,
                        &pref.lineworks_user_ids,
                    )
                    .await;
            }
        }
    }

    Ok(Json(ticket))
}

async fn delete_ticket(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let deleted = state
        .trouble_tickets
        .soft_delete(tenant.0 .0, id)
        .await
        .map_err(|e| {
            tracing::error!("delete_ticket error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn transition_ticket(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<TransitionRequest>,
) -> Result<Json<TroubleTicket>, StatusCode> {
    let tenant_id = tenant.0 .0;

    // 現在のチケットを取得
    let current = state
        .trouble_tickets
        .get(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // 遷移が許可されているか確認
    let allowed = state
        .trouble_workflow
        .is_transition_allowed(tenant_id, current.status_id, body.to_state_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !allowed {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    // ステータス更新
    let updated = state
        .trouble_tickets
        .update_status(tenant_id, id, body.to_state_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // 履歴記録
    let _ = state
        .trouble_workflow
        .record_history(
            tenant_id,
            id,
            current.status_id,
            body.to_state_id,
            None,
            body.comment,
        )
        .await;

    // webhook通知
    if let Some(webhook) = &state.webhook {
        let payload = serde_json::json!({
            "event": "trouble_status_changed",
            "timestamp": chrono::Utc::now(),
            "tenant_id": tenant_id,
            "data": {
                "ticket_id": id,
                "ticket_no": updated.ticket_no,
                "from_status_id": current.status_id,
                "to_status_id": body.to_state_id,
            }
        });
        webhook
            .fire_event(tenant_id, "trouble_status_changed", payload)
            .await;
    }

    // LINE WORKS Bot 通知
    if let Some(notifier) = &state.notifier {
        if let Ok(Some(pref)) = state
            .trouble_notification_prefs
            .find_enabled(tenant_id, "trouble_status_changed", "lineworks")
            .await
        {
            let msg = format!(
                "ステータス変更: #{} {}",
                updated.ticket_no, updated.category,
            );
            notifier
                .notify(
                    tenant_id,
                    "trouble_status_changed",
                    &msg,
                    &pref.lineworks_user_ids,
                )
                .await;
        }
    }

    Ok(Json(updated))
}

/// CSV出力 (BOM付き)
async fn export_csv(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<TroubleTicketFilter>,
) -> Result<impl IntoResponse, StatusCode> {
    // per_page を大きくして全件取得
    let big_filter = TroubleTicketFilter {
        per_page: Some(10000),
        page: Some(1),
        ..filter
    };
    let response = state
        .trouble_tickets
        .list(tenant.0 .0, &big_filter)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut wtr = csv::Writer::from_writer(vec![]);

    wtr.write_record([
        "No",
        "発生日",
        "所属会社名",
        "営業所名",
        "運行課",
        "当事者名",
        "登録番号",
        "事故等分類",
        "発生場所",
        "内容",
        "進捗状況",
        "手当等",
        "損害額",
        "賠償額",
        "確認書_決定通知書",
        "処分検討内容",
        "処分内容",
        "ロードサービス費用",
        "相手",
        "相手保険会社",
    ])
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    for t in &response.tickets {
        wtr.write_record([
            t.ticket_no.to_string(),
            t.occurred_date
                .map_or(String::new(), |d| d.format("%Y-%m-%d").to_string()),
            t.company_name.clone(),
            t.office_name.clone(),
            t.department.clone(),
            t.person_name.clone(),
            t.vehicle_number.clone(),
            t.category.clone(),
            t.location.clone(),
            t.description.clone(),
            t.progress_notes.clone(),
            t.allowance.clone(),
            t.damage_amount.clone().unwrap_or_default(),
            t.compensation_amount.clone().unwrap_or_default(),
            t.confirmation_notice.clone(),
            t.disciplinary_content.clone(),
            t.disciplinary_action.clone(),
            t.road_service_cost.clone().unwrap_or_default(),
            t.counterparty.clone(),
            t.counterparty_insurance.clone(),
        ])
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    let csv_bytes = wtr
        .into_inner()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // BOM + CSV
    let mut bom_csv = vec![0xEF, 0xBB, 0xBF];
    bom_csv.extend_from_slice(&csv_bytes);

    Ok((
        [
            (
                axum::http::header::CONTENT_TYPE,
                "text/csv; charset=utf-8".to_string(),
            ),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=\"trouble_tickets.csv\"".to_string(),
            ),
        ],
        bom_csv,
    ))
}

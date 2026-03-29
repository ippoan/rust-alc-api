use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::db::models::{
    CreateTimePunchByCard, CreateTimecardCard, TimePunchFilter, TimePunchWithEmployee,
    TimePunchesResponse, TimecardCard,
};
use crate::middleware::auth::TenantId;
use crate::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/timecard/cards", post(create_card).get(list_cards))
        .route("/timecard/cards/{id}", get(get_card).delete(delete_card))
        .route(
            "/timecard/cards/by-card/{card_id}",
            get(get_card_by_card_id),
        )
        .route("/timecard/punch", post(punch))
        .route("/timecard/punches", get(list_punches))
        .route("/timecard/punches/csv", get(export_csv))
}

// --- Card CRUD ---

async fn create_card(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateTimecardCard>,
) -> Result<(StatusCode, Json<TimecardCard>), StatusCode> {
    let tenant_id = tenant.0 .0;

    let card = state
        .timecard
        .create_card(
            tenant_id,
            body.employee_id,
            &body.card_id,
            body.label.as_deref(),
        )
        .await
        .map_err(|e| {
            tracing::error!("create_card error: {e}");
            if e.to_string().contains("idx_timecard_cards_unique") {
                return StatusCode::CONFLICT;
            }
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok((StatusCode::CREATED, Json(card)))
}

#[derive(Debug, serde::Deserialize)]
struct CardFilter {
    employee_id: Option<Uuid>,
}

async fn list_cards(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<CardFilter>,
) -> Result<Json<Vec<TimecardCard>>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let cards = state
        .timecard
        .list_cards(tenant_id, filter.employee_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(cards))
}

async fn get_card(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<TimecardCard>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let card = state
        .timecard
        .get_card(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(card))
}

async fn get_card_by_card_id(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(card_id): Path<String>,
) -> Result<Json<TimecardCard>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let card = state
        .timecard
        .get_card_by_card_id(tenant_id, &card_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(card))
}

async fn delete_card(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let tenant_id = tenant.0 .0;

    let deleted = state
        .timecard
        .delete_card(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !deleted {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

// --- Punch ---

async fn punch(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateTimePunchByCard>,
) -> Result<(StatusCode, Json<TimePunchWithEmployee>), StatusCode> {
    let tenant_id = tenant.0 .0;

    // カードIDから社員を特定 (timecard_cards -> employees.nfc_id フォールバック)
    let employee_id = if let Some(card) = state
        .timecard
        .find_card_by_card_id(tenant_id, &body.card_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        card.employee_id
    } else {
        // フォールバック: employees.nfc_id で検索
        state
            .timecard
            .find_employee_id_by_nfc(tenant_id, &body.card_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?
    };

    // 打刻記録
    let punch = state
        .timecard
        .create_punch(tenant_id, employee_id, body.device_id)
        .await
        .map_err(|e| {
            tracing::error!("punch error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // 社員名を取得
    let employee_name = state
        .timecard
        .get_employee_name(tenant_id, employee_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // 当日の打刻一覧
    let today_punches = state
        .timecard
        .list_today_punches(tenant_id, employee_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((
        StatusCode::CREATED,
        Json(TimePunchWithEmployee {
            punch,
            employee_name,
            today_punches,
        }),
    ))
}

// --- List Punches ---

async fn list_punches(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<TimePunchFilter>,
) -> Result<Json<TimePunchesResponse>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let per_page = filter.per_page.unwrap_or(50).min(200);
    let page = filter.page.unwrap_or(1).max(1);
    let offset = (page - 1) * per_page;

    let total = state
        .timecard
        .count_punches(
            tenant_id,
            filter.employee_id,
            filter.date_from,
            filter.date_to,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let punches = state
        .timecard
        .list_punches(
            tenant_id,
            filter.employee_id,
            filter.date_from,
            filter.date_to,
            per_page,
            offset,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(TimePunchesResponse {
        punches,
        total,
        page,
        per_page,
    }))
}

// --- CSV Export ---

async fn export_csv(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<TimePunchFilter>,
) -> Result<impl IntoResponse, StatusCode> {
    let tenant_id = tenant.0 .0;

    let rows = state
        .timecard
        .list_punches_for_csv(
            tenant_id,
            filter.employee_id,
            filter.date_from,
            filter.date_to,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut wtr = csv::Writer::from_writer(vec![]);
    wtr.write_record(["ID", "社員コード", "社員名", "打刻日時", "デバイス"])
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    for r in &rows {
        wtr.write_record([
            r.id.to_string(),
            r.employee_code.clone().unwrap_or_default(),
            r.employee_name.clone(),
            r.punched_at
                .with_timezone(&chrono::FixedOffset::east_opt(9 * 3600).unwrap())
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
            r.device_name.clone().unwrap_or_default(),
        ])
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    let csv_data = wtr
        .into_inner()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut output = vec![0xEF, 0xBB, 0xBF];
    output.extend_from_slice(&csv_data);

    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=\"time_punches.csv\"",
            ),
        ],
        output,
    ))
}

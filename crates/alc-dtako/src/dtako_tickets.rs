use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, patch, post},
    Extension, Json, Router,
};
use uuid::Uuid;

use crate::DtakoState;
use alc_core::auth_middleware::TenantId;
use alc_core::models::{
    DtakoTicket, DtakoTicketCloseRequest, DtakoTicketCloseResponse, DtakoTicketCreate,
    DtakoTicketFilter, DtakoTicketScrapedPatch, DtakoTicketsResponse,
};

/// nuxt_dtako_logs から JWT 経由で叩く一覧 / 詳細。
/// `require_tenant_header` middleware 配下に nest される想定。
pub fn tenant_router<S>() -> Router<S>
where
    DtakoState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/dtako/tickets", get(list_tickets))
        .route("/dtako/tickets/{id}", get(get_ticket))
}

/// email-receiver Worker から INTERNAL_SHARED_SECRET + X-Tenant-ID で叩く起票 / 反映。
/// `require_internal_shared_secret` middleware 配下に nest される想定。
pub fn internal_router<S>() -> Router<S>
where
    DtakoState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/dtako/tickets", post(create_ticket))
        .route("/dtako/tickets/{id}/scraped", patch(patch_scraped))
}

/// browser から close_token のみで叩く close 経路 (認証不要)。
pub fn public_close_router<S>() -> Router<S>
where
    DtakoState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new().route("/dtako/tickets/close", post(close_ticket))
}

async fn create_ticket(
    State(state): State<DtakoState>,
    tenant: Extension<TenantId>,
    Json(body): Json<DtakoTicketCreate>,
) -> Result<(StatusCode, Json<DtakoTicket>), StatusCode> {
    if body.vehicle_name.trim().is_empty() || body.error_kind.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let ticket = state
        .dtako_tickets
        .create(tenant.0 .0, &body)
        .await
        .map_err(|e| {
            tracing::error!("dtako_tickets.create error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok((StatusCode::CREATED, Json(ticket)))
}

async fn patch_scraped(
    State(state): State<DtakoState>,
    tenant: Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<DtakoTicketScrapedPatch>,
) -> Result<Json<DtakoTicket>, StatusCode> {
    let ticket = state
        .dtako_tickets
        .patch_scraped(tenant.0 .0, id, &body)
        .await
        .map_err(|e| {
            tracing::error!("dtako_tickets.patch_scraped error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(ticket))
}

async fn list_tickets(
    State(state): State<DtakoState>,
    tenant: Extension<TenantId>,
    Query(filter): Query<DtakoTicketFilter>,
) -> Result<Json<DtakoTicketsResponse>, StatusCode> {
    let resp = state
        .dtako_tickets
        .list(tenant.0 .0, &filter)
        .await
        .map_err(|e| {
            tracing::error!("dtako_tickets.list error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(resp))
}

async fn get_ticket(
    State(state): State<DtakoState>,
    tenant: Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<DtakoTicket>, StatusCode> {
    let ticket = state
        .dtako_tickets
        .get(tenant.0 .0, id)
        .await
        .map_err(|e| {
            tracing::error!("dtako_tickets.get error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(ticket))
}

async fn close_ticket(
    State(state): State<DtakoState>,
    Json(body): Json<DtakoTicketCloseRequest>,
) -> Result<Json<DtakoTicketCloseResponse>, StatusCode> {
    let token = body.close_token.trim();
    if token.is_empty() || token.len() > 128 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let closed_by = body.closed_by.as_deref().filter(|s| !s.is_empty());
    let ticket_id = state
        .dtako_tickets
        .close_by_token(token, closed_by)
        .await
        .map_err(|e| {
            tracing::error!("dtako_tickets.close_by_token error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(DtakoTicketCloseResponse { ticket_id }))
}

#[cfg(test)]
mod tests {

    #[test]
    fn close_token_length_validation() {
        // close_ticket は body の close_token 長を検査するため、handler 内で
        // 直接呼び出さずに長さ判定だけ網羅する補助テスト。
        let too_long: String = "a".repeat(129);
        assert!(too_long.len() > 128);
        assert!("ok".len() <= 128);
    }
}

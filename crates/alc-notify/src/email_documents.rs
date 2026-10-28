//! 文書の削除エンドポイント (テナント保護)。
//!
//! 受信メールの一覧・1 件と添付のダウンロード (`GET /notify/emails`・`/notify/emails/{message_id}`・
//! `/notify/documents/{id}/download`) は ippoan/alc-notify-worker へ移した (W8c-1、Refs #680)。削除は W8c-2 まで残る。

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Router,
};
use uuid::Uuid;

use alc_core::auth_middleware::TenantId;
use alc_core::tenant::TenantConn;
use alc_core::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new().route(
        "/notify/documents/{id}",
        axum::routing::delete(delete_document),
    )
}

async fn delete_document(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let mut tc = TenantConn::acquire(state.pool(), &tenant.0.to_string())
        .await
        .map_err(|e| {
            tracing::error!("acquire tenant conn: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // 先に r2_key を取得
    let row: Option<(String,)> =
        sqlx::query_as("SELECT r2_key FROM notify_documents WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tc.conn)
            .await
            .map_err(|e| {
                tracing::error!("lookup document: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    let r2_key = row.ok_or(StatusCode::NOT_FOUND)?.0;

    // DB から削除 (deliveries は ON DELETE CASCADE)
    sqlx::query("DELETE FROM notify_documents WHERE id = $1")
        .bind(id)
        .execute(&mut *tc.conn)
        .await
        .map_err(|e| {
            tracing::error!("delete document: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    drop(tc);

    // R2 からも削除 (best-effort)
    if let Some(storage) = state.notify_storage.as_ref() {
        if let Err(e) = storage.delete(&r2_key).await {
            tracing::warn!("notify_storage.delete (orphan ok): {e}");
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

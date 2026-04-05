//! 既読トラッキング
//! メッセージ内リンクをクリック → 既読記録 → フロントエンドにリダイレクト

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Router,
};
use uuid::Uuid;

use alc_core::AppState;

pub fn public_router() -> Router<AppState> {
    Router::new().route("/notify/read/{token}", axum::routing::get(read_redirect))
}

async fn read_redirect(
    State(state): State<AppState>,
    Path(token): Path<Uuid>,
) -> Result<Response, StatusCode> {
    let result = state
        .notify_deliveries
        .mark_read(token)
        .await
        .map_err(|e| {
            tracing::error!("mark_read: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    match result {
        Some(r) => {
            // フロントエンドのドキュメント詳細ページにリダイレクト
            let frontend_url = std::env::var("NOTIFY_FRONTEND_URL")
                .unwrap_or_else(|_| "https://notify.example.com".into());
            let redirect_url = format!("{}/documents/{}", frontend_url, r.document_id);

            Ok((StatusCode::FOUND, [(header::LOCATION, redirect_url)]).into_response())
        }
        None => {
            // 既に既読済み or トークン不正 → それでもリダイレクト
            let frontend_url = std::env::var("NOTIFY_FRONTEND_URL")
                .unwrap_or_else(|_| "https://notify.example.com".into());
            Ok((StatusCode::FOUND, [(header::LOCATION, frontend_url)]).into_response())
        }
    }
}

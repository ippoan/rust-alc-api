use axum::{
    extract::{Query, State},
    routing::get,
    Extension, Json, Router,
};
use chrono::NaiveDate;
use serde::Deserialize;

use crate::DtakoState;
use alc_core::auth_middleware::TenantId;

pub use alc_core::repository::dtako_scraper::ScrapeHistoryItem;

#[derive(Deserialize)]
pub struct HistoryQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

/// front Worker (nuxt-dtako-admin の dtako-scraper-relay) から、1 comp_id 分の
/// スクレイプ結果を記録するためのリクエスト。
///
/// dtako-scraper (Kagoya VPS) は GCP Cloud Run から到達不可能 (VPS の
/// `127.0.0.1` にしか bind されておらず、Cloud Run は Cloudflare Tunnel の
/// Private Network route に WARP client として乗れない) なため、SSE 中継は
/// front Worker + Durable Object (`workers/dtako-scraper-relay`) 側に移管した。
/// rust-alc-api は「履歴を保存するだけ」の薄い endpoint になる。
#[derive(Deserialize)]
pub struct ScrapeHistoryEntry {
    pub target_date: NaiveDate,
    pub comp_id: String,
    pub status: String,
    #[serde(default)]
    pub message: Option<String>,
}

async fn save_scrape_history(
    State(state): State<DtakoState>,
    Extension(tenant_id): Extension<TenantId>,
    Json(entry): Json<ScrapeHistoryEntry>,
) -> Result<axum::http::StatusCode, (axum::http::StatusCode, String)> {
    state
        .dtako_scraper
        .insert_scrape_history(
            tenant_id.0,
            entry.target_date,
            &entry.comp_id,
            &entry.status,
            entry.message.as_deref(),
        )
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("DB error: {e}"),
            )
        })?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// スクレイプ履歴を取得
async fn get_scrape_history(
    State(state): State<DtakoState>,
    Extension(tenant_id): Extension<TenantId>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<Vec<ScrapeHistoryItem>>, (axum::http::StatusCode, String)> {
    let rows = state
        .dtako_scraper
        .list_scrape_history(tenant_id.0, query.limit, query.offset)
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("DB error: {e}"),
            )
        })?;

    Ok(Json(rows))
}

pub fn tenant_router<S>() -> Router<S>
where
    DtakoState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new().route(
        "/scraper/history",
        get(get_scrape_history).post(save_scrape_history),
    )
}

use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

use alc_core::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(health_check))
}

async fn health_check() -> Json<Value> {
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION"), "service": "alc-api" }))
}

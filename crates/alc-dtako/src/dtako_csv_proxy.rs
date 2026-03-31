use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Serialize;

use alc_core::auth_middleware::TenantId;
use alc_core::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new().route("/operations/{unko_no}/csv/{csv_type}", get(get_csv_as_json))
}

#[derive(Debug, Serialize)]
pub struct CsvJsonResponse {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

async fn get_csv_as_json(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path((unko_no, csv_type)): Path<(String, String)>,
) -> Result<Json<CsvJsonResponse>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let filename = match csv_type.to_lowercase().as_str() {
        "kudguri" => "KUDGURI.csv",
        "kudgivt" | "events" => "KUDGIVT.csv",
        "kudgfry" | "ferry" | "ferries" => "KUDGFRY.csv",
        "kudgsir" | "tolls" => "KUDGSIR.csv",
        "speed" | "sokudo" => "SOKUDODATA.csv",
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let storage = state
        .dtako_storage
        .as_ref()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    let r2_prefix = state
        .dtako_csv_proxy
        .get_r2_key_prefix(tenant_id, &unko_no)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let key = match r2_prefix {
        Some(prefix) => format!("{}/{}", prefix, filename),
        None => format!("{}/unko/{}/{}", tenant_id, unko_no, filename),
    };
    tracing::info!("CSV download: key={}", key);

    let bytes = storage.download(&key).await.map_err(|e| {
        tracing::error!("CSV download failed: key={}, error={}", key, e);
        StatusCode::NOT_FOUND
    })?;

    let text = String::from_utf8_lossy(&bytes);
    let mut lines = text.lines();

    let headers: Vec<String> = lines
        .next()
        .unwrap_or("")
        .split(',')
        .map(|h| h.trim().to_string())
        .collect();

    let rows: Vec<Vec<String>> = lines
        .filter(|l| !l.trim().is_empty())
        .map(|line| line.split(',').map(|f| f.trim().to_string()).collect())
        .collect();

    Ok(Json(CsvJsonResponse { headers, rows }))
}

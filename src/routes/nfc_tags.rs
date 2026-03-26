use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::TenantId;
use crate::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/nfc-tags", get(list_tags).post(register_tag))
        .route("/nfc-tags/search", get(search_by_uuid))
        .route("/nfc-tags/{nfc_uuid}", delete(delete_tag))
}

#[derive(Debug, Clone, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NfcTag {
    pub id: i32,
    pub nfc_uuid: String,
    pub car_inspection_id: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

fn normalize_nfc_uuid(uuid: &str) -> String {
    uuid.to_lowercase().replace(':', "")
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    uuid: String,
}

#[derive(Debug, Serialize)]
struct SearchResponse {
    nfc_tag: NfcTag,
    car_inspection: Option<serde_json::Value>,
}

async fn search_by_uuid(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<SearchResponse>, StatusCode> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let nfc_uuid = normalize_nfc_uuid(&q.uuid);

    let tag = sqlx::query_as::<_, NfcTag>(
        "SELECT id, nfc_uuid, car_inspection_id, created_at FROM car_inspection_nfc_tags WHERE nfc_uuid = $1",
    )
    .bind(&nfc_uuid)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    // Get linked car inspection
    let ci = sqlx::query_as::<_, (serde_json::Value,)>(
        "SELECT to_jsonb(ci) FROM car_inspection ci WHERE id = $1",
    )
    .bind(tag.car_inspection_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .map(|r| r.0);

    Ok(Json(SearchResponse {
        nfc_tag: tag,
        car_inspection: ci,
    }))
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    car_inspection_id: Option<i32>,
}

async fn list_tags(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<NfcTag>>, StatusCode> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows = if let Some(ci_id) = q.car_inspection_id {
        sqlx::query_as::<_, NfcTag>(
            "SELECT id, nfc_uuid, car_inspection_id, created_at FROM car_inspection_nfc_tags WHERE car_inspection_id = $1 ORDER BY created_at DESC",
        )
        .bind(ci_id)
        .fetch_all(&mut *conn)
        .await
    } else {
        sqlx::query_as::<_, NfcTag>(
            "SELECT id, nfc_uuid, car_inspection_id, created_at FROM car_inspection_nfc_tags ORDER BY created_at DESC",
        )
        .fetch_all(&mut *conn)
        .await
    }
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
struct RegisterRequest {
    nfc_uuid: String,
    car_inspection_id: i32,
}

async fn register_tag(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
    Json(body): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<NfcTag>), StatusCode> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let nfc_uuid = normalize_nfc_uuid(&body.nfc_uuid);

    let tag = sqlx::query_as::<_, NfcTag>(
        r#"
        INSERT INTO car_inspection_nfc_tags (tenant_id, nfc_uuid, car_inspection_id)
        VALUES (current_setting('app.current_tenant_id')::uuid, $1, $2)
        ON CONFLICT (tenant_id, nfc_uuid) DO UPDATE
            SET car_inspection_id = EXCLUDED.car_inspection_id,
                created_at = NOW()
        RETURNING id, nfc_uuid, car_inspection_id, created_at
        "#,
    )
    .bind(&nfc_uuid)
    .bind(body.car_inspection_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("register_tag failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::CREATED, Json(tag)))
}

async fn delete_tag(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
    Path(nfc_uuid): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let normalized = normalize_nfc_uuid(&nfc_uuid);

    let result = sqlx::query("DELETE FROM car_inspection_nfc_tags WHERE nfc_uuid = $1")
        .bind(&normalized)
        .execute(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::NO_CONTENT)
}

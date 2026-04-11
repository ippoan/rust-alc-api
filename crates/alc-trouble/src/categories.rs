use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};
use uuid::Uuid;

use crate::TroubleState;
use alc_core::auth_middleware::TenantId;
use alc_core::models::{CreateTroubleCategory, TroubleCategory};

pub fn tenant_router<S>() -> Router<S>
where
    TroubleState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/trouble/categories",
            get(list_categories).post(create_category),
        )
        .route(
            "/trouble/categories/{id}",
            delete(delete_category).put(update_category_sort),
        )
}

async fn list_categories(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<TroubleCategory>>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let categories = state
        .trouble_categories
        .list(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !categories.is_empty() {
        return Ok(Json(categories));
    }

    // Auto-seed default categories
    let mut seeded = Vec::new();
    for (i, name) in crate::DEFAULT_CATEGORIES.iter().enumerate() {
        let input = CreateTroubleCategory {
            name: name.to_string(),
            sort_order: Some(i as i32 + 1),
        };
        match state.trouble_categories.create(tenant_id, &input).await {
            Ok(cat) => seeded.push(cat),
            Err(e) => {
                tracing::warn!("auto-seed category {name}: {e}");
            }
        }
    }
    Ok(Json(seeded))
}

async fn create_category(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateTroubleCategory>,
) -> Result<(StatusCode, Json<TroubleCategory>), StatusCode> {
    if body.name.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let category = state
        .trouble_categories
        .create(tenant.0 .0, &body)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e {
                if db_err.constraint().is_some() {
                    return StatusCode::CONFLICT;
                }
            }
            tracing::error!("create_category error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok((StatusCode::CREATED, Json(category)))
}

#[derive(Debug, serde::Deserialize)]
struct UpdateSortOrder {
    sort_order: i32,
}

async fn update_category_sort(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateSortOrder>,
) -> Result<Json<TroubleCategory>, StatusCode> {
    let cat = state
        .trouble_categories
        .update_sort_order(tenant.0 .0, id, body.sort_order)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(cat))
}

async fn delete_category(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let deleted = state
        .trouble_categories
        .delete(tenant.0 .0, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

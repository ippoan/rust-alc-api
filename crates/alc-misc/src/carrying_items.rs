use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use serde::Serialize;
use uuid::Uuid;

use alc_core::auth_middleware::TenantId;
use alc_core::models::{
    CarryingItem, CarryingItemVehicleCondition, CreateCarryingItem, UpdateCarryingItem,
};
use alc_core::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/carrying-items", get(list_items).post(create_item))
        .route("/carrying-items/{id}", put(update_item).delete(delete_item))
}

#[derive(Debug, Serialize)]
struct CarryingItemWithConditions {
    #[serde(flatten)]
    item: CarryingItem,
    vehicle_conditions: Vec<CarryingItemVehicleCondition>,
}

async fn list_items(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<CarryingItemWithConditions>>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let items = state
        .carrying_items
        .list(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let item_ids: Vec<Uuid> = items.iter().map(|i| i.id).collect();

    let conditions = if item_ids.is_empty() {
        vec![]
    } else {
        state
            .carrying_items
            .list_conditions(tenant_id, &item_ids)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    let result = items
        .into_iter()
        .map(|item| {
            let conds: Vec<_> = conditions
                .iter()
                .filter(|c| c.carrying_item_id == item.id)
                .cloned()
                .collect();
            CarryingItemWithConditions {
                item,
                vehicle_conditions: conds,
            }
        })
        .collect();

    Ok(Json(result))
}

async fn create_item(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateCarryingItem>,
) -> Result<(StatusCode, Json<CarryingItemWithConditions>), StatusCode> {
    let tenant_id = tenant.0 .0;

    let item = state
        .carrying_items
        .create(
            tenant_id,
            &body.item_name,
            body.is_required.unwrap_or(true),
            body.sort_order.unwrap_or(0),
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut conditions = Vec::new();
    for vc in &body.vehicle_conditions {
        let cond = state
            .carrying_items
            .insert_condition(tenant_id, item.id, &vc.category, &vc.value)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if let Some(c) = cond {
            conditions.push(c);
        }
    }

    Ok((
        StatusCode::CREATED,
        Json(CarryingItemWithConditions {
            item,
            vehicle_conditions: conditions,
        }),
    ))
}

async fn update_item(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateCarryingItem>,
) -> Result<Json<CarryingItemWithConditions>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let item = state
        .carrying_items
        .update(
            tenant_id,
            id,
            body.item_name.as_deref(),
            body.is_required,
            body.sort_order,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // vehicle_conditions が指定された場合は全置換
    let conditions = if let Some(vcs) = &body.vehicle_conditions {
        state
            .carrying_items
            .delete_conditions(tenant_id, id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let mut conds = Vec::new();
        for vc in vcs {
            let cond = state
                .carrying_items
                .insert_condition(tenant_id, id, &vc.category, &vc.value)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            if let Some(c) = cond {
                conds.push(c);
            }
        }
        conds
    } else {
        state
            .carrying_items
            .get_conditions(tenant_id, id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    Ok(Json(CarryingItemWithConditions {
        item,
        vehicle_conditions: conditions,
    }))
}

async fn delete_item(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let tenant_id = tenant.0 .0;

    // ON DELETE CASCADE で conditions も消える
    let deleted = state
        .carrying_items
        .delete(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !deleted {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use serde::Serialize;
use uuid::Uuid;

use crate::db::models::{
    CarryingItem, CarryingItemVehicleCondition, CreateCarryingItem, UpdateCarryingItem,
};
use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::TenantId;
use crate::AppState;

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
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let items = sqlx::query_as::<_, CarryingItem>(
        "SELECT * FROM alc_api.carrying_items ORDER BY sort_order, created_at",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let item_ids: Vec<Uuid> = items.iter().map(|i| i.id).collect();

    let conditions = if item_ids.is_empty() {
        vec![]
    } else {
        sqlx::query_as::<_, CarryingItemVehicleCondition>(
            "SELECT * FROM alc_api.carrying_item_vehicle_conditions WHERE carrying_item_id = ANY($1) ORDER BY category, value",
        )
        .bind(&item_ids)
        .fetch_all(&mut *conn)
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
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let item = sqlx::query_as::<_, CarryingItem>(
        r#"INSERT INTO alc_api.carrying_items (tenant_id, item_name, is_required, sort_order)
           VALUES ($1, $2, $3, $4)
           RETURNING *"#,
    )
    .bind(tenant_id)
    .bind(&body.item_name)
    .bind(body.is_required.unwrap_or(true))
    .bind(body.sort_order.unwrap_or(0))
    .fetch_one(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut conditions = Vec::new();
    for vc in &body.vehicle_conditions {
        let cond = sqlx::query_as::<_, CarryingItemVehicleCondition>(
            r#"INSERT INTO alc_api.carrying_item_vehicle_conditions (carrying_item_id, category, value)
               VALUES ($1, $2, $3)
               ON CONFLICT (carrying_item_id, category, value) DO NOTHING
               RETURNING *"#,
        )
        .bind(item.id)
        .bind(&vc.category)
        .bind(&vc.value)
        .fetch_optional(&mut *conn)
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
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let item = sqlx::query_as::<_, CarryingItem>(
        r#"UPDATE alc_api.carrying_items SET
               item_name = COALESCE($1, item_name),
               is_required = COALESCE($2, is_required),
               sort_order = COALESCE($3, sort_order)
           WHERE id = $4
           RETURNING *"#,
    )
    .bind(&body.item_name)
    .bind(body.is_required)
    .bind(body.sort_order)
    .bind(id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    // vehicle_conditions が指定された場合は全置換
    let conditions = if let Some(vcs) = &body.vehicle_conditions {
        sqlx::query(
            "DELETE FROM alc_api.carrying_item_vehicle_conditions WHERE carrying_item_id = $1",
        )
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let mut conds = Vec::new();
        for vc in vcs {
            let cond = sqlx::query_as::<_, CarryingItemVehicleCondition>(
                r#"INSERT INTO alc_api.carrying_item_vehicle_conditions (carrying_item_id, category, value)
                   VALUES ($1, $2, $3)
                   ON CONFLICT (carrying_item_id, category, value) DO NOTHING
                   RETURNING *"#,
            )
            .bind(id)
            .bind(&vc.category)
            .bind(&vc.value)
            .fetch_optional(&mut *conn)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            if let Some(c) = cond {
                conds.push(c);
            }
        }
        conds
    } else {
        sqlx::query_as::<_, CarryingItemVehicleCondition>(
            "SELECT * FROM alc_api.carrying_item_vehicle_conditions WHERE carrying_item_id = $1 ORDER BY category, value",
        )
        .bind(id)
        .fetch_all(&mut *conn)
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
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // ON DELETE CASCADE で conditions も消える
    let result = sqlx::query("DELETE FROM alc_api.carrying_items WHERE id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

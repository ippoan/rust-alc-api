use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Extension, Json, Router,
};
use serde::Serialize;

use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::TenantId;
use crate::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/car-inspections/current", get(list_current))
        .route("/car-inspections/expired", get(list_expired))
        .route("/car-inspections/renew", get(list_renew))
        .route(
            "/car-inspections/vehicle-categories",
            get(vehicle_categories),
        )
        .route("/car-inspections/{id}", get(get_by_id))
}

#[derive(Debug, Serialize)]
struct ListResponse {
    #[serde(rename = "carInspections")]
    car_inspections: Vec<serde_json::Value>,
}

async fn list_current(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
) -> Result<Json<ListResponse>, StatusCode> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows = sqlx::query_as::<_, (serde_json::Value,)>(
        r#"
        SELECT to_jsonb(sub) FROM (
            SELECT DISTINCT ON (ci."CarId")
                ci.*,
                (SELECT uuid::text FROM car_inspection_files_b
                 WHERE tenant_id = ci.tenant_id
                   AND "ElectCertMgNo" = ci."ElectCertMgNo"
                   AND "GrantdateE" = ci."GrantdateE"
                   AND "GrantdateY" = ci."GrantdateY"
                   AND "GrantdateM" = ci."GrantdateM"
                   AND "GrantdateD" = ci."GrantdateD"
                   AND type = 'application/pdf'
                   AND deleted_at IS NULL
                 ORDER BY created_at DESC LIMIT 1) as "pdfUuid",
                (SELECT uuid::text FROM car_inspection_files_a
                 WHERE tenant_id = ci.tenant_id
                   AND "ElectCertMgNo" = ci."ElectCertMgNo"
                   AND "GrantdateE" = ci."GrantdateE"
                   AND "GrantdateY" = ci."GrantdateY"
                   AND "GrantdateM" = ci."GrantdateM"
                   AND "GrantdateD" = ci."GrantdateD"
                   AND type = 'application/json'
                   AND deleted_at IS NULL
                 ORDER BY created_at DESC LIMIT 1) as "jsonUuid"
            FROM car_inspection ci
            ORDER BY ci."CarId",
                     ci."TwodimensionCodeInfoValidPeriodExpirdate" DESC,
                     ci.created_at DESC
        ) sub
        "#,
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("list_current failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(ListResponse {
        car_inspections: rows.into_iter().map(|r| r.0).collect(),
    }))
}

async fn get_by_id(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
    Path(id): Path<i32>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let row = sqlx::query_as::<_, (serde_json::Value,)>(
        "SELECT to_jsonb(ci) FROM car_inspection ci WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("get_by_id failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(row.0))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct VehicleCategories {
    car_kinds: Vec<String>,
    uses: Vec<String>,
    car_shapes: Vec<String>,
    private_businesses: Vec<String>,
}

async fn vehicle_categories(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
) -> Result<Json<VehicleCategories>, StatusCode> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let row = sqlx::query_as::<_, VehicleCategories>(
        r#"SELECT
            COALESCE(ARRAY(SELECT DISTINCT "CarKind" FROM alc_api.car_inspection WHERE "CarKind" != '' ORDER BY "CarKind"), '{}') AS car_kinds,
            COALESCE(ARRAY(SELECT DISTINCT "Use" FROM alc_api.car_inspection WHERE "Use" != '' ORDER BY "Use"), '{}') AS uses,
            COALESCE(ARRAY(SELECT DISTINCT "CarShape" FROM alc_api.car_inspection WHERE "CarShape" != '' ORDER BY "CarShape"), '{}') AS car_shapes,
            COALESCE(ARRAY(SELECT DISTINCT "PrivateBusiness" FROM alc_api.car_inspection WHERE "PrivateBusiness" != '' ORDER BY "PrivateBusiness"), '{}') AS private_businesses
        "#,
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("vehicle_categories failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(row))
}

async fn list_expired(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
) -> Result<Json<ListResponse>, StatusCode> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows = sqlx::query_as::<_, (serde_json::Value,)>(
        r#"
        SELECT to_jsonb(ci)
        FROM car_inspection ci
        WHERE "TwodimensionCodeInfoValidPeriodExpirdate" <= to_char(CURRENT_DATE + INTERVAL '30 days', 'YYMMDD')
        ORDER BY "TwodimensionCodeInfoValidPeriodExpirdate" ASC
        "#,
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("list_expired failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(ListResponse {
        car_inspections: rows.into_iter().map(|r| r.0).collect(),
    }))
}

async fn list_renew(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<TenantId>,
) -> Result<Json<ListResponse>, StatusCode> {
    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.0.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows = sqlx::query_as::<_, (serde_json::Value,)>(
        r#"
        SELECT to_jsonb(ci)
        FROM car_inspection ci
        WHERE "TwodimensionCodeInfoValidPeriodExpirdate" >= to_char(CURRENT_DATE, 'YYMMDD')
          AND "TwodimensionCodeInfoValidPeriodExpirdate" <= to_char(CURRENT_DATE + INTERVAL '60 days', 'YYMMDD')
        ORDER BY "TwodimensionCodeInfoValidPeriodExpirdate" ASC
        "#,
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("list_renew failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(ListResponse {
        car_inspections: rows.into_iter().map(|r| r.0).collect(),
    }))
}

use axum::{extract::State, http::StatusCode, routing::get, Extension, Json, Router};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::TenantId;
use crate::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new().route("/car-inspection-files/current", get(list_current))
}

#[derive(Debug, Clone, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CarInspectionFile {
    pub uuid: Uuid,
    #[sqlx(rename = "type")]
    pub file_type: String,
    #[sqlx(rename = "ElectCertMgNo")]
    pub elect_cert_mg_no: String,
    #[sqlx(rename = "GrantdateE")]
    pub grantdate_e: String,
    #[sqlx(rename = "GrantdateY")]
    pub grantdate_y: String,
    #[sqlx(rename = "GrantdateM")]
    pub grantdate_m: String,
    #[sqlx(rename = "GrantdateD")]
    pub grantdate_d: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub modified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize)]
struct ListResponse {
    files: Vec<CarInspectionFile>,
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

    let rows = sqlx::query_as::<_, CarInspectionFile>(
        r#"
        SELECT cif.*
        FROM car_inspection_files_a cif
        INNER JOIN car_inspection ci ON
            cif."ElectCertMgNo" = ci."ElectCertMgNo"
            AND cif."GrantdateE" = ci."GrantdateE"
            AND cif."GrantdateY" = ci."GrantdateY"
            AND cif."GrantdateM" = ci."GrantdateM"
            AND cif."GrantdateD" = ci."GrantdateD"
        WHERE cif.deleted_at IS NULL
          AND ci."TwodimensionCodeInfoValidPeriodExpirdate" >= to_char(CURRENT_DATE, 'YYMMDD')
        ORDER BY cif.created_at DESC
        "#,
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| {
        tracing::error!("list_current_car_inspection_files failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(ListResponse { files: rows }))
}

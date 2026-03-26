use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::Response,
    routing::get,
    Json, Router,
};
use uuid::Uuid;

use crate::db::models::{TenkoRecord, TenkoRecordFilter, TenkoRecordsResponse};
use crate::db::tenant::set_current_tenant;
use crate::middleware::auth::TenantId;
use crate::AppState;

/// テナント対応ルート (JWT or X-Tenant-ID)
pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/tenko/records", get(list_records))
        .route("/tenko/records/csv", get(export_csv))
        .route("/tenko/records/{id}", get(get_record))
}

async fn list_records(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<TenkoRecordFilter>,
) -> Result<Json<TenkoRecordsResponse>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let per_page = filter.per_page.unwrap_or(50).min(100);
    let page = filter.page.unwrap_or(1).max(1);
    let offset = (page - 1) * per_page;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut conditions = vec!["r.tenant_id = $1".to_string()];
    let mut param_idx = 2u32;

    if filter.employee_id.is_some() {
        conditions.push(format!("r.employee_id = ${param_idx}"));
        param_idx += 1;
    }
    if filter.tenko_type.is_some() {
        conditions.push(format!("r.tenko_type = ${param_idx}"));
        param_idx += 1;
    }
    if filter.status.is_some() {
        conditions.push(format!("r.status = ${param_idx}"));
        param_idx += 1;
    }
    if filter.date_from.is_some() {
        conditions.push(format!("r.recorded_at >= ${param_idx}"));
        param_idx += 1;
    }
    if filter.date_to.is_some() {
        conditions.push(format!("r.recorded_at <= ${param_idx}"));
        param_idx += 1;
    }

    let where_clause = conditions.join(" AND ");

    let count_sql = format!("SELECT COUNT(*) FROM tenko_records r WHERE {where_clause}");
    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql).bind(tenant_id);
    if let Some(employee_id) = filter.employee_id {
        count_query = count_query.bind(employee_id);
    }
    if let Some(ref tenko_type) = filter.tenko_type {
        count_query = count_query.bind(tenko_type);
    }
    if let Some(ref status) = filter.status {
        count_query = count_query.bind(status);
    }
    if let Some(date_from) = filter.date_from {
        count_query = count_query.bind(date_from);
    }
    if let Some(date_to) = filter.date_to {
        count_query = count_query.bind(date_to);
    }
    let total = count_query
        .fetch_one(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let sql = format!(
        "SELECT r.* FROM tenko_records r WHERE {where_clause} ORDER BY r.recorded_at DESC LIMIT ${param_idx} OFFSET ${}",
        param_idx + 1
    );
    let mut query = sqlx::query_as::<_, TenkoRecord>(&sql).bind(tenant_id);
    if let Some(employee_id) = filter.employee_id {
        query = query.bind(employee_id);
    }
    if let Some(ref tenko_type) = filter.tenko_type {
        query = query.bind(tenko_type);
    }
    if let Some(ref status) = filter.status {
        query = query.bind(status);
    }
    if let Some(date_from) = filter.date_from {
        query = query.bind(date_from);
    }
    if let Some(date_to) = filter.date_to {
        query = query.bind(date_to);
    }
    query = query.bind(per_page).bind(offset);

    let records = query
        .fetch_all(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(TenkoRecordsResponse {
        records,
        total,
        page,
        per_page,
    }))
}

async fn get_record(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<TenkoRecord>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let record = sqlx::query_as::<_, TenkoRecord>(
        "SELECT * FROM tenko_records WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(record))
}

/// CSV エクスポート
async fn export_csv(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<TenkoRecordFilter>,
) -> Result<Response, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut conn = state
        .pool
        .acquire()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    set_current_tenant(&mut conn, &tenant_id.to_string())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // フィルタ構築
    let mut conditions = vec!["r.tenant_id = $1".to_string()];
    let mut param_idx = 2u32;

    if filter.employee_id.is_some() {
        conditions.push(format!("r.employee_id = ${param_idx}"));
        param_idx += 1;
    }
    if filter.tenko_type.is_some() {
        conditions.push(format!("r.tenko_type = ${param_idx}"));
        param_idx += 1;
    }
    if filter.status.is_some() {
        conditions.push(format!("r.status = ${param_idx}"));
        param_idx += 1;
    }
    if filter.date_from.is_some() {
        conditions.push(format!("r.recorded_at >= ${param_idx}"));
        param_idx += 1;
    }
    if filter.date_to.is_some() {
        conditions.push(format!("r.recorded_at <= ${param_idx}"));
        let _ = param_idx;
    }

    let where_clause = conditions.join(" AND ");
    let sql =
        format!("SELECT r.* FROM tenko_records r WHERE {where_clause} ORDER BY r.recorded_at DESC");

    let mut query = sqlx::query_as::<_, TenkoRecord>(&sql).bind(tenant_id);
    if let Some(employee_id) = filter.employee_id {
        query = query.bind(employee_id);
    }
    if let Some(ref tenko_type) = filter.tenko_type {
        query = query.bind(tenko_type);
    }
    if let Some(ref status) = filter.status {
        query = query.bind(status);
    }
    if let Some(date_from) = filter.date_from {
        query = query.bind(date_from);
    }
    if let Some(date_to) = filter.date_to {
        query = query.bind(date_to);
    }

    let records = query
        .fetch_all(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // CSV 生成
    let mut wtr = csv::Writer::from_writer(vec![]);

    wtr.write_record([
        "record_id",
        "employee_name",
        "tenko_type",
        "tenko_method",
        "status",
        "responsible_manager_name",
        "started_at",
        "completed_at",
        "location",
        "alcohol_result",
        "alcohol_value",
        "alcohol_has_face_photo",
        "temperature",
        "systolic",
        "diastolic",
        "pulse",
        "instruction",
        "instruction_confirmed_at",
        "report_vehicle_road_status",
        "report_driver_alternation",
        "report_no_report",
        "self_declaration_illness",
        "self_declaration_fatigue",
        "self_declaration_sleep",
        "safety_judgment_status",
        "safety_judgment_failed_items",
        "daily_inspection_status",
        "interrupted_at",
        "resumed_at",
        "resume_reason",
        "recorded_at",
        "record_hash",
    ])
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    for r in &records {
        // Phase 2: JSONB フィールドから値を抽出
        let (decl_illness, decl_fatigue, decl_sleep) = r
            .self_declaration
            .as_ref()
            .map(|v| {
                (
                    v.get("illness")
                        .and_then(|b| b.as_bool())
                        .map_or(String::new(), |b| b.to_string()),
                    v.get("fatigue")
                        .and_then(|b| b.as_bool())
                        .map_or(String::new(), |b| b.to_string()),
                    v.get("sleep_deprivation")
                        .and_then(|b| b.as_bool())
                        .map_or(String::new(), |b| b.to_string()),
                )
            })
            .unwrap_or_default();

        let (judgment_status, judgment_items) = r
            .safety_judgment
            .as_ref()
            .map(|v| {
                let status = v
                    .get("status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string();
                let items = v
                    .get("failed_items")
                    .and_then(|a| a.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|i| i.as_str())
                            .collect::<Vec<_>>()
                            .join(";")
                    })
                    .unwrap_or_default();
                (status, items)
            })
            .unwrap_or_default();

        let inspection_status = r
            .daily_inspection
            .as_ref()
            .map(|v| {
                let items = [
                    "brakes",
                    "tires",
                    "lights",
                    "steering",
                    "wipers",
                    "mirrors",
                    "horn",
                    "seatbelts",
                ];
                let has_ng = items
                    .iter()
                    .any(|k| v.get(k).and_then(|s| s.as_str()) == Some("ng"));
                if has_ng {
                    "ng".to_string()
                } else {
                    "ok".to_string()
                }
            })
            .unwrap_or_default();

        wtr.write_record([
            r.id.to_string(),
            r.employee_name.clone(),
            r.tenko_type.clone(),
            r.tenko_method.clone(),
            r.status.clone(),
            r.responsible_manager_name.clone(),
            r.started_at.map_or(String::new(), |t| t.to_rfc3339()),
            r.completed_at.map_or(String::new(), |t| t.to_rfc3339()),
            r.location.clone().unwrap_or_default(),
            r.alcohol_result.clone().unwrap_or_default(),
            r.alcohol_value.map_or(String::new(), |v| v.to_string()),
            r.alcohol_has_face_photo.to_string(),
            r.temperature.map_or(String::new(), |v| v.to_string()),
            r.systolic.map_or(String::new(), |v| v.to_string()),
            r.diastolic.map_or(String::new(), |v| v.to_string()),
            r.pulse.map_or(String::new(), |v| v.to_string()),
            r.instruction.clone().unwrap_or_default(),
            r.instruction_confirmed_at
                .map_or(String::new(), |t| t.to_rfc3339()),
            r.report_vehicle_road_status.clone().unwrap_or_default(),
            r.report_driver_alternation.clone().unwrap_or_default(),
            r.report_no_report.map_or(String::new(), |v| v.to_string()),
            decl_illness,
            decl_fatigue,
            decl_sleep,
            judgment_status,
            judgment_items,
            inspection_status,
            r.interrupted_at.map_or(String::new(), |t| t.to_rfc3339()),
            r.resumed_at.map_or(String::new(), |t| t.to_rfc3339()),
            r.resume_reason.clone().unwrap_or_default(),
            r.recorded_at.to_rfc3339(),
            r.record_hash.clone(),
        ])
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    let data = wtr
        .into_inner()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // BOM 付き UTF-8 (Excel 対応)
    let mut bom_data = vec![0xEF, 0xBB, 0xBF];
    bom_data.extend_from_slice(&data);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"tenko_records.csv\"",
        )
        .body(Body::from(bom_data))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

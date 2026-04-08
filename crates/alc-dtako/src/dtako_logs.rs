use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};

use alc_core::auth_middleware::TenantId;
use alc_core::models::{
    BulkUpsertResponse, DtakologDateQuery, DtakologDateRangeQuery, DtakologInput,
    DtakologSelectQuery, DtakologView,
};
use alc_core::AppState;

pub fn tenant_router() -> Router<AppState> {
    Router::new()
        .route("/current", get(current_list_all))
        .route("/by-date", get(get_by_date))
        .route("/current/select", get(current_list_select))
        .route("/by-date-range", get(get_by_date_range))
        .route("/bulk", post(bulk_upsert))
}

async fn current_list_all(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<DtakologView>>, StatusCode> {
    let rows = state
        .dtako_logs
        .current_list_all(tenant.0 .0)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.into_iter().map(DtakologView::from).collect()))
}

async fn get_by_date(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(q): Query<DtakologDateQuery>,
) -> Result<Json<Vec<DtakologView>>, StatusCode> {
    let rows = state
        .dtako_logs
        .get_date(tenant.0 .0, &q.date_time, q.vehicle_cd)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.into_iter().map(DtakologView::from).collect()))
}

async fn current_list_select(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(q): Query<DtakologSelectQuery>,
) -> Result<Json<Vec<DtakologView>>, StatusCode> {
    let vehicle_cds: Vec<i32> = q
        .vehicle_cds
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let rows = state
        .dtako_logs
        .current_list_select(
            tenant.0 .0,
            q.address_disp_p.as_deref(),
            q.branch_cd,
            &vehicle_cds,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.into_iter().map(DtakologView::from).collect()))
}

async fn get_by_date_range(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Query(q): Query<DtakologDateRangeQuery>,
) -> Result<Json<Vec<DtakologView>>, StatusCode> {
    // DB から取得 (直近7日分)
    let mut rows = state
        .dtako_logs
        .get_date_range(
            tenant.0 .0,
            &q.start_date_time,
            &q.end_date_time,
            q.vehicle_cd,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // R2 アーカイブからも取得 (dtako_storage が設定されている場合)
    if let Some(ref storage) = state.dtako_storage {
        let start_date = extract_date(&q.start_date_time);
        let end_date = extract_date(&q.end_date_time);
        let tenant_str = tenant.0 .0.to_string();
        // fetch_from_r2 handles all errors internally (returns Ok(vec![]))
        if let Ok(r2_rows) = crate::archive_reader::fetch_from_r2(
            storage.as_ref(),
            &tenant_str,
            &start_date,
            &end_date,
            q.vehicle_cd,
        )
        .await
        {
            rows.extend(r2_rows);
            rows.sort_by(|a, b| a.data_date_time.cmp(&b.data_date_time));
            rows.dedup_by(|a, b| {
                a.data_date_time == b.data_date_time && a.vehicle_cd == b.vehicle_cd
            });
        }
    }

    Ok(Json(rows.into_iter().map(DtakologView::from).collect()))
}

/// 日時文字列から YYYY-MM-DD 部分を抽出
fn extract_date(datetime_str: &str) -> String {
    // "2026-04-07T12:00:00" or "2026-04-07" → "2026-04-07"
    if datetime_str.len() >= 10 && datetime_str.as_bytes()[4] == b'-' {
        return datetime_str[..10].to_string();
    }
    // Fallback: return first 10 chars (e.g. "26/04/07 12:00" → "26/04/07 1")
    datetime_str.chars().take(10).collect()
}

async fn bulk_upsert(
    State(state): State<AppState>,
    tenant: axum::Extension<TenantId>,
    Json(records): Json<Vec<DtakologInput>>,
) -> Result<Json<BulkUpsertResponse>, StatusCode> {
    if records.is_empty() {
        return Ok(Json(BulkUpsertResponse {
            success: true,
            records_added: 0,
            total_records: 0,
            message: "No records provided".to_string(),
        }));
    }

    let total = records.len() as i32;
    let affected = state
        .dtako_logs
        .bulk_upsert(tenant.0 .0, &records)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(BulkUpsertResponse {
        success: true,
        records_added: affected as i32,
        total_records: total,
        message: format!("Upserted {} records", affected),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_date_iso_datetime() {
        assert_eq!(extract_date("2026-04-07T12:00:00"), "2026-04-07");
    }

    #[test]
    fn extract_date_space_datetime() {
        assert_eq!(extract_date("2026-04-07 12:00:00"), "2026-04-07");
    }

    #[test]
    fn extract_date_already_date() {
        assert_eq!(extract_date("2026-04-07"), "2026-04-07");
    }

    #[test]
    fn extract_date_short_input() {
        assert_eq!(extract_date("abc"), "abc");
    }

    #[test]
    fn extract_date_non_dash_format_short() {
        // Less than 10 chars, non-standard format → fallback to take(10)
        assert_eq!(extract_date("26/04/07"), "26/04/07");
    }

    #[test]
    fn extract_date_long_non_iso() {
        // 10+ chars but byte[4] != '-' → chrono parse fails → take(10) fallback
        assert_eq!(extract_date("26/04/07 12:00:00"), "26/04/07 1");
    }
}

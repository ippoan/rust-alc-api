//! Y時間 export JSON エンドポイント。
//!
//! `GET /api/dtako/y-time-export?driver_cd=X&from=YYYY-MM-DD&to=YYYY-MM-DD`
//!
//! 1. driver_cd → employees.id を解決
//! 2. dtako_operations から期間内 (`reading_date ± 1 day`) の運行を列挙
//! 3. 各 unko_no について R2 から KUDGIVT.csv を都度 fetch
//! 4. `split_by_rest` で segment 化、event_cd=301 を sum して rest_minutes 算出
//! 5. 1 暦日 2 始業 ルールで bucketing
//! 6. JSON で返す
//!
//! xlsx 生成は frontend Worker 側 (nuxt-dtako-admin) で行う。

pub mod builder;
pub mod csv_aggregator;
pub mod models;

use crate::DtakoState;
use alc_core::auth_middleware::TenantId;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};

use builder::{build_y_time_rows, SegmentInput};
use csv_aggregator::{build_segment_inputs, fetch_and_parse_kudgivt};
use models::{YTimeDriver, YTimeExportQuery, YTimeExportResponse, YTimePeriod};

pub fn tenant_router<S>() -> Router<S>
where
    DtakoState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new().route("/dtako/y-time-export", get(get_y_time_export))
}

async fn get_y_time_export(
    State(state): State<DtakoState>,
    tenant: axum::Extension<TenantId>,
    Query(q): Query<YTimeExportQuery>,
) -> Result<Json<YTimeExportResponse>, (StatusCode, String)> {
    let tenant_id = tenant.0 .0;

    if q.from > q.to {
        return Err((StatusCode::BAD_REQUEST, "from > to".to_string()));
    }

    // 1. driver_cd lookup
    let (driver_id, driver_name) = state
        .dtako_y_time_export
        .lookup_driver(tenant_id, &q.driver_cd)
        .await
        .map_err(|e| {
            tracing::error!("lookup_driver error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal error".to_string(),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("driver_cd not found: {}", q.driver_cd),
            )
        })?;

    // 2. 期間内の運行列挙
    let operations = state
        .dtako_y_time_export
        .list_operations(tenant_id, driver_id, q.from, q.to)
        .await
        .map_err(|e| {
            tracing::error!("list_operations error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal error".to_string(),
            )
        })?;

    let storage = state
        .dtako_storage
        .as_ref()
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "dtako storage not configured".to_string(),
            )
        })?
        .clone();

    // 3-4. 各 unko_no について CSV 取得 + segment 化 + 301 sum
    let mut all_segments: Vec<SegmentInput> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    for op in operations {
        let (departure_at, return_at) = match (op.departure_at, op.return_at) {
            (Some(d), Some(r)) => (d, r),
            _ => {
                warnings.push(format!(
                    "{}: departure_at/return_at が不足、skip",
                    op.unko_no
                ));
                continue;
            }
        };

        let events = match fetch_and_parse_kudgivt(
            storage.as_ref(),
            tenant_id,
            &op.unko_no,
            op.r2_key_prefix.as_deref(),
            op.crew_role,
        )
        .await
        {
            Ok(e) => e,
            Err(err) => {
                tracing::warn!("fetch KUDGIVT failed unko_no={} err={}", op.unko_no, err);
                warnings.push(format!("{}: KUDGIVT 取得失敗 ({})", op.unko_no, err));
                continue;
            }
        };

        let segs = build_segment_inputs(&events, naive(departure_at), naive(return_at));
        all_segments.extend(segs);
    }

    // 5. bucketing
    let (rows, build_warnings) = build_y_time_rows(all_segments, q.from, q.to);
    warnings.extend(build_warnings);

    Ok(Json(YTimeExportResponse {
        driver: YTimeDriver {
            cd: q.driver_cd,
            name: driver_name,
        },
        period: YTimePeriod {
            from: q.from,
            to: q.to,
        },
        rows,
        warnings,
    }))
}

/// `DateTime<Utc>` → `NaiveDateTime` (TZ なし wall clock 抽出)。
///
/// dtako_operations.departure_at は TIMESTAMPTZ だが、upload pipeline が JST wall-clock を
/// そのまま UTC として保存している (DB 層のタイムゾーン補正なし)。よって
/// `naive_utc()` で時刻部だけ取り出せば JST wall clock として扱える。
fn naive(dt: DateTime<Utc>) -> chrono::NaiveDateTime {
    dt.naive_utc()
}

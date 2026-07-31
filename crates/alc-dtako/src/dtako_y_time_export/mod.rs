//! Y時間 export JSON エンドポイント。
//!
//! `GET /api/dtako/y-time-export?driver_cd=X&from=YYYY-MM-DD&to=YYYY-MM-DD`
//!
//! 1. driver_cd → employees.id を解決
//! 2. dtako_operations から期間内 (`reading_date` **または** `operation_date` が
//!    `± 1 day` 広げた範囲に入る) の運行を列挙
//! 3. 各 unko_no について R2 から KUDGIVT.csv を **並列 fetch** (buffer_unordered 16)
//! 4. `split_by_rest` で segment 化、event_cd=301 を sum して rest_minutes 算出
//! 5. 1 暦日 2 始業 ルールで bucketing
//! 6. JSON で返す
//!
//! xlsx 生成は frontend Worker 側 (nuxt-dtako-admin) で行う。
//!
//! ## 設計補足: 同期 GET だけにした経緯 (2026-05-10)
//!
//! 一時期 `POST /jobs` + WebSocket 完了通知 (notify-realtime-bus) で async job 化を
//! 試みたが、**Cloud Run の CPU throttling (default ON) により `tokio::spawn` した
//! background compute が HTTP 200/202 後に CPU 停止 → 完走しない**ことが発覚し撤回した。
//!
//! 撤回判断:
//! - parallel R2 fetch だけで 41-107s → 5-15s に短縮 (Cloudflare proxy 100s timeout 内)
//! - async pattern は `--no-cpu-throttling` (instance-based billing、月 ~$60 増) か
//!   Cloud Tasks queue / DurableObject compute への移行が必要
//! - 5-15s なら sync HTTP で十分、複雑性に見合うリターンなし
//!
//! 将来 async pattern を再導入するなら:
//! - Cloud Run `--no-cpu-throttling` を deploy.sh に固定 + コスト承認
//! - もしくは Cloudflare DurableObject 内で compute (R2 binding native、ただし alc-csv-parser
//!   の TS/WASM 移植 + DB query 分離が必要)
//! - もしくは Cloud Tasks 経由の別 worker サービス
//!
//! `crates/alc-core/src/realtime_bus.rs` の `RealtimeBus` 汎用 client は将来用に残してある。

pub mod builder;
pub mod csv_aggregator;
pub mod models;

use crate::DtakoState;
use alc_core::auth_middleware::TenantId;
use alc_core::storage::StorageBackend;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use futures::stream::StreamExt;
use std::sync::Arc;
use uuid::Uuid;

use builder::{build_y_time_rows, SegmentInput};
use csv_aggregator::{build_segment_inputs, fetch_and_parse_kudgivt};
use models::{YTimeDriver, YTimeExportQuery, YTimeExportResponse, YTimePeriod};

/// R2 から KUDGIVT.csv を並列 fetch する際の同時実行数。
///
/// 13ヶ月レンジで unko_no が 100-200 件、1 fetch ~300ms とすると 200/16 ≈ 3.75 秒で完了する想定。
/// R2 rate limit 安全圏、TCP socket 枯渇懸念なし。
const R2_FETCH_CONCURRENCY: usize = 16;

pub fn tenant_router<S>() -> Router<S>
where
    DtakoState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new().route("/dtako/y-time-export", get(get_y_time_export))
}

/// 同期 GET。compute 完了まで HTTP を保持する (5-15s 想定、Cloudflare proxy 100s 内)。
async fn get_y_time_export(
    State(state): State<DtakoState>,
    tenant: axum::Extension<TenantId>,
    Query(q): Query<YTimeExportQuery>,
) -> Result<Json<YTimeExportResponse>, (StatusCode, String)> {
    let tenant_id = tenant.0 .0;
    let resp = compute_y_time_export(&state, tenant_id, q)
        .await
        .map_err(compute_error_to_response)?;
    Ok(Json(resp))
}

/// 共通 compute コア。テストはこの関数単位で書ける。
///
/// `get_y_time_export` ハンドラ + 将来の async 系 (DO compute / Cloud Tasks 等)
/// から再利用可能なように分離してある。
pub async fn compute_y_time_export(
    state: &DtakoState,
    tenant_id: Uuid,
    q: YTimeExportQuery,
) -> Result<YTimeExportResponse, ComputeError> {
    if q.from > q.to {
        return Err(ComputeError::BadRequest("from > to".to_string()));
    }

    // 1. driver_cd lookup
    let (driver_id, driver_name) = state
        .dtako_y_time_export
        .lookup_driver(tenant_id, &q.driver_cd)
        .await
        .map_err(|e| {
            tracing::error!("lookup_driver error: {e}");
            ComputeError::Internal("internal error".to_string())
        })?
        .ok_or_else(|| ComputeError::NotFound(format!("driver_cd not found: {}", q.driver_cd)))?;

    // 2. 期間内の運行列挙
    let operations = state
        .dtako_y_time_export
        .list_operations(tenant_id, driver_id, q.from, q.to)
        .await
        .map_err(|e| {
            tracing::error!("list_operations error: {e}");
            ComputeError::Internal("internal error".to_string())
        })?;

    let storage = state
        .dtako_storage
        .as_ref()
        .ok_or_else(|| ComputeError::Internal("dtako storage not configured".to_string()))?
        .clone();

    // 3. 各 unko_no について R2 から CSV 並列 fetch
    let fetch_started = std::time::Instant::now();
    let mut warnings: Vec<String> = Vec::new();

    // 3-A. 欠落チェックを sync で先に潰し、有効 op だけ並列対象に残す。
    //      順序保持のため warning も先に push (旧逐次実装と同じ順番)
    let mut targets: Vec<FetchTarget> = Vec::with_capacity(operations.len());
    for op in operations {
        match (op.departure_at, op.return_at) {
            (Some(d), Some(r)) => {
                targets.push(FetchTarget {
                    unko_no: op.unko_no,
                    r2_key_prefix: op.r2_key_prefix,
                    crew_role: op.crew_role,
                    departure_at: d,
                    return_at: r,
                });
            }
            _ => {
                warnings.push(format!(
                    "{}: departure_at/return_at が不足、skip",
                    op.unko_no
                ));
            }
        }
    }

    let target_count = targets.len();

    // 3-B. 並列 fetch (buffer_unordered で R2_FETCH_CONCURRENCY 件まで concurrent)
    let storage_for_fetch: Arc<dyn StorageBackend> = storage.clone();
    let fetched: Vec<FetchResult> = futures::stream::iter(targets.into_iter().map(|t| {
        let storage_inner = storage_for_fetch.clone();
        async move {
            let res = fetch_and_parse_kudgivt(
                storage_inner.as_ref(),
                tenant_id,
                &t.unko_no,
                t.r2_key_prefix.as_deref(),
                t.crew_role,
            )
            .await;
            FetchResult {
                target: t,
                outcome: res,
            }
        }
    }))
    .buffer_unordered(R2_FETCH_CONCURRENCY)
    .collect()
    .await;

    let fetch_elapsed_ms = fetch_started.elapsed().as_millis();
    tracing::info!(
        targets = target_count,
        concurrency = R2_FETCH_CONCURRENCY,
        fetch_elapsed_ms,
        "y-time-export: parallel R2 fetch done"
    );

    // 3-C. sync 後処理: ok→segment 化 + extend、err→warning push
    let mut all_segments: Vec<SegmentInput> = Vec::with_capacity(target_count);
    for FetchResult { target, outcome } in fetched {
        match outcome {
            Ok(events) => {
                let segs = build_segment_inputs(
                    &events,
                    naive(target.departure_at),
                    naive(target.return_at),
                );
                all_segments.extend(segs);
            }
            Err(err) => {
                tracing::warn!(
                    "fetch KUDGIVT failed unko_no={} err={}",
                    target.unko_no,
                    err
                );
                warnings.push(format!("{}: KUDGIVT 取得失敗 ({})", target.unko_no, err));
            }
        }
    }

    // 5. bucketing
    let (rows, build_warnings) = build_y_time_rows(all_segments, q.from, q.to);
    warnings.extend(build_warnings);

    Ok(YTimeExportResponse {
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
    })
}

/// `compute_y_time_export` の戻り値型。HTTP 層で `(StatusCode, String)` に変換する。
#[derive(Debug)]
pub enum ComputeError {
    BadRequest(String),
    NotFound(String),
    Internal(String),
}

impl std::fmt::Display for ComputeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadRequest(m) | Self::NotFound(m) | Self::Internal(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for ComputeError {}

/// `ComputeError` → HTTP。`dtako_events` も同じ写像を使う (Refs rust-ichibanboshi#205)。
pub fn compute_error_to_response(err: ComputeError) -> (StatusCode, String) {
    match err {
        ComputeError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
        ComputeError::NotFound(m) => (StatusCode::NOT_FOUND, m),
        ComputeError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
    }
}

/// 並列 fetch のための target tuple。
struct FetchTarget {
    unko_no: String,
    r2_key_prefix: Option<String>,
    crew_role: i32,
    departure_at: DateTime<Utc>,
    return_at: DateTime<Utc>,
}

/// 並列 fetch 結果と元 target をペアで保持。
struct FetchResult {
    target: FetchTarget,
    outcome: Result<Vec<alc_csv_parser::kudgivt::KudgivtRow>, csv_aggregator::AggregatorError>,
}

/// `DateTime<Utc>` → `NaiveDateTime` (TZ なし wall clock 抽出)。
///
/// dtako_operations.departure_at は TIMESTAMPTZ だが、upload pipeline が JST wall-clock を
/// そのまま UTC として保存している (DB 層のタイムゾーン補正なし)。よって
/// `naive_utc()` で時刻部だけ取り出せば JST wall clock として扱える。
fn naive(dt: DateTime<Utc>) -> chrono::NaiveDateTime {
    dt.naive_utc()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_error_response_mapping() {
        assert_eq!(
            compute_error_to_response(ComputeError::BadRequest("bad".into())),
            (StatusCode::BAD_REQUEST, "bad".to_string())
        );
        assert_eq!(
            compute_error_to_response(ComputeError::NotFound("nope".into())),
            (StatusCode::NOT_FOUND, "nope".to_string())
        );
        assert_eq!(
            compute_error_to_response(ComputeError::Internal("oops".into())),
            (StatusCode::INTERNAL_SERVER_ERROR, "oops".to_string())
        );
    }

    #[test]
    fn compute_error_display_passthrough() {
        assert_eq!(ComputeError::BadRequest("x".into()).to_string(), "x");
        assert_eq!(ComputeError::NotFound("y".into()).to_string(), "y");
        assert_eq!(ComputeError::Internal("z".into()).to_string(), "z");
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn r2_fetch_concurrency_is_in_safe_range() {
        // 16 でなくてもいいが、過大設定回帰を防ぐ。
        // 1 (= 逐次) は禁止、200+ は R2 rate limit リスク。
        assert!(R2_FETCH_CONCURRENCY >= 4);
        assert!(R2_FETCH_CONCURRENCY <= 64);
    }
}

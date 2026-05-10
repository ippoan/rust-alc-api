//! Y時間 export JSON エンドポイント。
//!
//! 同期 GET と async job (POST + WS 完了通知) の 2 系統を提供する。
//!
//! - **同期 GET** `GET /api/dtako/y-time-export?driver_cd=X&from=YYYY-MM-DD&to=YYYY-MM-DD`
//!   従来通り compute → JSON return。Cloud Run wall time が長い (13ヶ月で 5-15s)
//!   ため Cloudflare proxy 経由 frontend からは推奨しない。後方互換のため残置。
//!
//! - **async job** `POST /api/dtako/y-time-export/jobs?driver_cd=X&from=...&to=...`
//!   即時 `202 { job_id }` を返し、background tokio::spawn で compute → 完了時に
//!   `realtime_bus` (notify-realtime-bus Worker) へ result を inline broadcast。
//!   frontend (`useYTimeExportJob` composable) が `Sec-WebSocket-Protocol: bearer,<jwt>`
//!   で `wss://realtime.../subscribe` を listen し、`kind="y_time_export"` &
//!   `job_id` 一致のイベントで result を受け取る。HTTP の長時間保持を回避し、
//!   202 から WS event 到着まで通常 5-15 秒。
//!
//! 共通 compute 部:
//! 1. driver_cd → employees.id を解決
//! 2. dtako_operations から期間内 (`reading_date ± 1 day`) の運行を列挙
//! 3. 各 unko_no について R2 から KUDGIVT.csv を **並列 fetch** (buffer_unordered 16)
//! 4. `split_by_rest` で segment 化、event_cd=301 を sum して rest_minutes 算出
//! 5. 1 暦日 2 始業 ルールで bucketing
//!
//! xlsx 生成は frontend Worker 側 (nuxt-dtako-admin) で行う。

pub mod builder;
pub mod csv_aggregator;
pub mod models;

use crate::DtakoState;
use alc_core::auth_middleware::TenantId;
use alc_core::storage::StorageBackend;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use futures::stream::StreamExt;
use serde::Serialize;
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
    Router::new()
        .route("/dtako/y-time-export", get(get_y_time_export))
        .route("/dtako/y-time-export/jobs", post(post_y_time_export_job))
}

/// 同期 GET: 後方互換用。compute 完了まで HTTP を保持する。
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

/// async job: 即 202 を返して background で compute → realtime_bus へ result を broadcast。
///
/// `realtime_bus` 未設定 (env vars 欠落) の場合、silent に compute だけ走って
/// result が行方不明になるのを避けるため `503` を返す。
async fn post_y_time_export_job(
    State(state): State<DtakoState>,
    tenant: axum::Extension<TenantId>,
    Query(q): Query<YTimeExportQuery>,
) -> Result<(StatusCode, Json<StartJobResponse>), (StatusCode, String)> {
    let tenant_id = tenant.0 .0;

    // 即時バリデーション (compute 内でも再度確認するが、202 を返してから報告するより
    // 400 で即落としたほうが UX 良い)
    if q.from > q.to {
        return Err((StatusCode::BAD_REQUEST, "from > to".to_string()));
    }

    // realtime_bus がないと job 完了通知ができない → 503 で失敗を返す
    let bus = state.realtime_bus.clone().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "realtime_bus not configured (NOTIFY_REDACT_BROADCAST_URL/SECRET 未設定)".to_string(),
    ))?;

    let job_id = Uuid::new_v4();
    tracing::info!(
        job_id = %job_id,
        tenant_id = %tenant_id,
        driver_cd = %q.driver_cd,
        "y-time-export: job spawned"
    );

    let bg_state = state.clone();
    let bg_query = q.clone();
    tokio::spawn(async move {
        let started = std::time::Instant::now();
        let outcome = compute_y_time_export(&bg_state, tenant_id, bg_query).await;
        let elapsed_ms = started.elapsed().as_millis();

        let event = match outcome {
            Ok(result) => {
                tracing::info!(
                    job_id = %job_id,
                    rows = result.rows.len(),
                    warnings = result.warnings.len(),
                    elapsed_ms,
                    "y-time-export: job completed"
                );
                YTimeJobEvent {
                    kind: "y_time_export",
                    tenant_id,
                    document_id: job_id,
                    job_id,
                    status: "completed",
                    result: Some(result),
                    error: None,
                }
            }
            Err(err) => {
                let msg = compute_error_to_message(&err);
                tracing::warn!(
                    job_id = %job_id,
                    error = %msg,
                    elapsed_ms,
                    "y-time-export: job failed"
                );
                YTimeJobEvent {
                    kind: "y_time_export",
                    tenant_id,
                    document_id: job_id,
                    job_id,
                    status: "failed",
                    result: None,
                    error: Some(msg),
                }
            }
        };
        bus.broadcast(&event).await;
    });

    Ok((StatusCode::ACCEPTED, Json(StartJobResponse { job_id })))
}

/// 同期 / async 共通の compute コア。テストはこの関数単位で書く。
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

fn compute_error_to_response(err: ComputeError) -> (StatusCode, String) {
    match err {
        ComputeError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
        ComputeError::NotFound(m) => (StatusCode::NOT_FOUND, m),
        ComputeError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
    }
}

fn compute_error_to_message(err: &ComputeError) -> String {
    err.to_string()
}

/// async job 起動時の即時レスポンス。
#[derive(Debug, Clone, Serialize)]
pub struct StartJobResponse {
    pub job_id: Uuid,
}

/// realtime-bus Worker `/broadcast` に送る Y時間 export 用イベント。
///
/// Worker は `tenant_id` / `document_id` / `status` を必須要求するため、
/// `document_id` には `job_id` (UUID) を入れて満たす。frontend は `kind` &
/// `job_id` で disambiguate する。
///
/// `result` は完了時のみ Some、288 KB 程度までの JSON を inline で運ぶ。
#[derive(Debug, Clone, Serialize)]
pub struct YTimeJobEvent {
    pub kind: &'static str,
    pub tenant_id: Uuid,
    /// Worker validation を通すため必須。`job_id` と同値を入れる。
    pub document_id: Uuid,
    pub job_id: Uuid,
    /// `completed` | `failed`
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<YTimeExportResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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
    fn compute_error_message_passthrough() {
        assert_eq!(
            compute_error_to_message(&ComputeError::BadRequest("x".into())),
            "x"
        );
        assert_eq!(
            compute_error_to_message(&ComputeError::NotFound("y".into())),
            "y"
        );
        assert_eq!(
            compute_error_to_message(&ComputeError::Internal("z".into())),
            "z"
        );
    }

    #[test]
    fn y_time_job_event_serialization_includes_kind_and_job_id() {
        let job_id = Uuid::nil();
        let tenant_id = Uuid::nil();
        let ev = YTimeJobEvent {
            kind: "y_time_export",
            tenant_id,
            document_id: job_id,
            job_id,
            status: "failed",
            result: None,
            error: Some("oops".to_string()),
        };
        let json = serde_json::to_string(&ev).expect("serialize");
        assert!(
            json.contains("\"kind\":\"y_time_export\""),
            "kind discriminator missing in {json}"
        );
        assert!(
            json.contains("\"job_id\""),
            "job_id field missing in {json}"
        );
        assert!(
            json.contains("\"document_id\""),
            "document_id field required by realtime-bus Worker missing in {json}"
        );
        assert!(
            json.contains("\"status\":\"failed\""),
            "status missing in {json}"
        );
        // result が None のときは skip_serializing_if で消える
        assert!(
            !json.contains("\"result\""),
            "result should be omitted when None: {json}"
        );
    }

    #[test]
    fn start_job_response_shape() {
        let r = StartJobResponse {
            job_id: Uuid::nil(),
        };
        let json = serde_json::to_string(&r).expect("serialize");
        assert_eq!(
            json,
            "{\"job_id\":\"00000000-0000-0000-0000-000000000000\"}"
        );
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

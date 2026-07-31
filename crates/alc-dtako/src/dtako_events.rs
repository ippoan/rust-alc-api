//! 月次 dtako 生イベント read endpoint (Refs ohishi-exp/rust-ichibanboshi#205 実装計画 01)。
//!
//! ```text
//! GET /api/dtako/events?driver_cd=X&date_from=YYYY-MM-DD&date_to=YYYY-MM-DD   (1 乗務員)
//! GET /api/dtako/events?date_from=YYYY-MM-DD&date_to=YYYY-MM-DD               (全乗務員)
//! ```
//!
//! 1. 対象乗務員を決める (`driver_cd` 指定 → 1 名 / 省略 → 期間内に運行のある全乗務員)
//! 2. `dtako_operations` から期間内 (`reading_date ± 1 day`、`has_kudgivt = TRUE`) の運行を列挙
//! 3. R2 の KUDGIVT.csv を **重複排除して並列 fetch** (buffer_unordered 16)
//! 4. **畳まずに** CSV の headers / rows をそのまま JSON で返す
//!
//! ## 既存 endpoint との関係
//!
//! `dtako_csv_proxy.rs` の `GET /operations/{unko_no}/csv/{csv_type}` は 1 運行 1 往復。
//! 1 乗務員 1 か月分を集めるのに N 回の HTTP が要り、これが #199 の cold 24〜33 秒の主因。
//! 本 endpoint は同じ内容を **1 リクエスト**で返す。1 運行分のペイロード形
//! (`{headers, rows}`) は per-運行 proxy と揃えてあるので、呼び出し側は列マッピングを
//! 使い回せる。
//!
//! ## ここで一切やらないこと
//!
//! 勤怠・拘束の計算 (`kosoku.rs`) は一番星固有のロジックで、マルチテナント基盤である
//! rust-alc-api には置かない。#205 の決定 3「`kosoku.rs` は写さない」がこれを指す。
//! よって本 endpoint は **イベント種別の分類・状態 (始業/終業/運行開始/…) への写像・
//! 時刻のパースや TZ 変換・勤務境界の判定・集計・`kintai_events` 相当への正規化を
//! 一切行わない**。加える手は
//!
//! - Shift-JIS フォールバック付きのデコード (R2 上の古いデータ対策)
//! - `,` split
//!
//! だけ。行の取捨選択もしない。
//!
//! KUDGIVT.csv は 1 運行分の全乗務員 (運転手 = 1 / 副運転手 = 2) の行を持つが、行を落とす
//! 判断も呼び出し側に委ねる。`operations[].crew_role` (= `dtako_operations.crew_role`) を
//! 添えてあるので、呼び出し側は CSV の `対象乗務員区分` 列と突き合わせて絞れる。
//!
//! ## headers を運行ごとに持つ理由
//!
//! per-unko の KUDGIVT.csv は upload された ZIP をそのまま split したもので、ヘッダは
//! **upload 時点のデジタコ出力に依存する**。実際 `対象乗務員CD` 列のように、ある時期の
//! ファイルにしか無い列が存在する (Refs #205 実装計画 08)。全運行で同一という保証が無いので
//! トップレベルに畳まず、`{unko_no, headers, rows}` を並べる形にする。既存
//! `get_csv_as_json` の `{headers, rows}` をそのまま束ねた形でもある。
//!
//! ## 全乗務員版と応答サイズ (#205 の 04 / 05 が全乗務員を回すため)
//!
//! #205 本文の 2026-01〜07 実測から、全乗務員 1 か月は運行NO 約 1,128 件 =
//! **R2 GET 約 1,128 回**、生 CSV 行に直すと約 66,000 行 = **JSON 17〜19 MB**。
//! Cloud Run の非ストリーミング応答上限 32 MiB に対して余裕が無い。よって
//!
//! - `driver_cd` 省略時は期間上限を **31 日** に落とす (366 日だと R2 GET が約 13,500 回、
//!   concurrency 16 で 4 分超、応答は数百 MB になり破綻する)
//! - **乗務員単位で keyset ページング** する (`page_size` / `after_driver_cd`)。
//!   1 乗務員が 2 ページに割れることは無いので、ページ単体で畳める
//!
//! ## 期間が ±1 日広がる
//!
//! 運行の列挙は `reading_date` を前後 1 日ずつ広げて引く (暦日を跨ぐ運行の取りこぼし防止)。
//! よって応答の `operations[].departure_at` が `date_from`/`date_to` の外側に出ることがある。
//! #205 の再計算は「差分日 ± 2 日」を対象にするので、広い方に倒しておくのが正しい。

use crate::dtako_y_time_export::csv_aggregator::build_kudgivt_key;
use crate::dtako_y_time_export::{compute_error_to_response, ComputeError};
use crate::DtakoState;
use alc_core::auth_middleware::TenantId;
use alc_core::repository::dtako_y_time_export::{DtakoDriverOperation, YTimeExportOperation};
use alc_core::storage::StorageBackend;
use alc_csv_parser::decode_shift_jis;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, NaiveDate, Utc};
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// R2 から KUDGIVT.csv を並列 fetch する際の同時実行数。
/// `dtako_y_time_export` と同じ根拠 (1 fetch ~300ms、R2 rate limit 安全圏)。
const R2_FETCH_CONCURRENCY: usize = 16;

/// `driver_cd` 指定時の期間上限 (日)。1 乗務員 1 年分 ≒ R2 GET 300 回程度。
const MAX_RANGE_DAYS_SINGLE: i64 = 366;

/// `driver_cd` 省略時 (全乗務員) の期間上限 (日)。1 か月 ≒ R2 GET 1,128 回。
/// 暦月はどれも 31 日以内なので「月次 read endpoint」の用途は満たす。
const MAX_RANGE_DAYS_ALL: i64 = 31;

/// 全乗務員版 1 ページあたりの乗務員数の既定値と上限。
/// 25 名 ≒ 4 MB、50 名 ≒ 8 MB (Cloud Run 応答上限 32 MiB に対する安全域)。
const DEFAULT_PAGE_SIZE: i64 = 25;
const MAX_PAGE_SIZE: i64 = 50;

/// `GET /api/dtako/events/etags` 専用の期間上限 (日)。
/// `MAX_RANGE_DAYS_ALL` (31) は CSV ダウンロード枚数 (R2 GET 回数・JSON 応答サイズ) が
/// 根拠だが、この endpoint は R2 LIST 1 回だけで GET を一切行わず、LIST のコストは
/// `date_from`/`date_to` に依存しない (フィルタは DB 側で掛ける)。よってその根拠は
/// 転用できず、別の定数にしてある。呼び出し側 (rust-ichibanboshi) の
/// `month_range(month)` は暦月 31 日 + 翌月またぎ猶予 1 日 = 最大 32 日なので、
/// それに余裕を持たせた値にする。
const MAX_RANGE_DAYS_ETAGS: i64 = 40;

pub fn tenant_router<S>() -> Router<S>
where
    DtakoState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/dtako/events", get(get_dtako_events))
        .route("/dtako/events/etags", get(get_dtako_events_etags))
}

#[derive(Debug, Clone, Deserialize)]
pub struct DtakoEventsQuery {
    /// 省略すると期間内に運行のある全乗務員が対象になる。
    pub driver_cd: Option<String>,
    pub date_from: NaiveDate,
    pub date_to: NaiveDate,
    /// 全乗務員版のみ有効。1 ページあたりの乗務員数 (既定 25、上限 50 に clamp)。
    pub page_size: Option<i64>,
    /// 全乗務員版のみ有効。この `driver_cd` より後ろから返す (排他的下限)。
    pub after_driver_cd: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DtakoEventsDriver {
    pub cd: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DtakoEventsPeriod {
    pub date_from: NaiveDate,
    pub date_to: NaiveDate,
}

/// 1 運行分の生 CSV。`headers` / `rows` の形は per-運行 proxy (`dtako_csv_proxy`) と同一。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DtakoEventsOperation {
    pub unko_no: String,
    /// `dtako_operations.crew_role`。CSV の `対象乗務員区分` 列と突き合わせて
    /// 呼び出し側が行を絞るためのメタデータ (ここでは絞らない)。
    pub crew_role: i32,
    pub departure_at: Option<DateTime<Utc>>,
    pub return_at: Option<DateTime<Utc>>,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// 全乗務員版の 1 乗務員分。単一乗務員版の `driver` + `operations` と同じ形。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DtakoEventsDriverGroup {
    pub driver: DtakoEventsDriver,
    pub operations: Vec<DtakoEventsOperation>,
}

/// `driver_cd` 指定時の応答。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DtakoEventsSingleResponse {
    pub driver: DtakoEventsDriver,
    pub period: DtakoEventsPeriod,
    pub operations: Vec<DtakoEventsOperation>,
    /// 取得失敗した運行を **握り潰さずに** 返す。応答が空なのか一部欠けたのかを
    /// 呼び出し側が区別できるようにするため。
    pub warnings: Vec<String>,
}

/// `driver_cd` 省略時の応答。`drivers[]` の各要素が単一乗務員版と同じ形。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DtakoEventsAllResponse {
    pub period: DtakoEventsPeriod,
    pub drivers: Vec<DtakoEventsDriverGroup>,
    /// 次ページの `after_driver_cd`。`null` なら最終ページ。
    pub next_after_driver_cd: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(untagged)]
pub enum DtakoEventsResponse {
    Single(Box<DtakoEventsSingleResponse>),
    All(Box<DtakoEventsAllResponse>),
}

async fn get_dtako_events(
    State(state): State<DtakoState>,
    tenant: axum::Extension<TenantId>,
    Query(q): Query<DtakoEventsQuery>,
) -> Result<Json<DtakoEventsResponse>, (StatusCode, String)> {
    let tenant_id = tenant.0 .0;
    let resp = collect_dtako_events(&state, tenant_id, q)
        .await
        .map_err(compute_error_to_response)?;
    Ok(Json(resp))
}

/// 共通 compute コア。handler と分離してテストしやすくしてある
/// (`compute_y_time_export` と同じ構成)。
pub async fn collect_dtako_events(
    state: &DtakoState,
    tenant_id: Uuid,
    q: DtakoEventsQuery,
) -> Result<DtakoEventsResponse, ComputeError> {
    let period = DtakoEventsPeriod {
        date_from: q.date_from,
        date_to: q.date_to,
    };
    let storage: Arc<dyn StorageBackend> = state
        .dtako_storage
        .as_ref()
        .ok_or_else(|| ComputeError::Internal("dtako storage not configured".to_string()))?
        .clone();

    match q.driver_cd.clone() {
        Some(driver_cd) => {
            validate_range(q.date_from, q.date_to, MAX_RANGE_DAYS_SINGLE)?;
            reject_paging_params(q.page_size, q.after_driver_cd.as_deref())?;
            let resp =
                collect_single_driver(state, &storage, tenant_id, driver_cd, period, &q).await?;
            Ok(DtakoEventsResponse::Single(Box::new(resp)))
        }
        None => {
            validate_range(q.date_from, q.date_to, MAX_RANGE_DAYS_ALL)?;
            let resp = collect_all_drivers(state, &storage, tenant_id, period, &q).await?;
            Ok(DtakoEventsResponse::All(Box::new(resp)))
        }
    }
}

async fn collect_single_driver(
    state: &DtakoState,
    storage: &Arc<dyn StorageBackend>,
    tenant_id: Uuid,
    driver_cd: String,
    period: DtakoEventsPeriod,
    q: &DtakoEventsQuery,
) -> Result<DtakoEventsSingleResponse, ComputeError> {
    let (driver_id, driver_name) = state
        .dtako_y_time_export
        .lookup_driver(tenant_id, &driver_cd)
        .await
        .map_err(|e| internal("lookup_driver", e))?
        .ok_or_else(|| ComputeError::NotFound(format!("driver_cd not found: {driver_cd}")))?;

    let operations = state
        .dtako_y_time_export
        .list_operations(tenant_id, driver_id, q.date_from, q.date_to)
        .await
        .map_err(|e| internal("list_operations", e))?;

    let mut plan = FetchPlan::default();
    let planned: Vec<PlannedOp> = operations
        .into_iter()
        .map(|op| PlannedOp::from_single(tenant_id, op, &mut plan))
        .collect();

    let fetched = fetch_all(storage, plan.keys).await;
    let (operations, warnings) = assemble(planned, &fetched);

    Ok(DtakoEventsSingleResponse {
        driver: DtakoEventsDriver {
            cd: driver_cd,
            name: driver_name,
        },
        period,
        operations,
        warnings,
    })
}

async fn collect_all_drivers(
    state: &DtakoState,
    storage: &Arc<dyn StorageBackend>,
    tenant_id: Uuid,
    period: DtakoEventsPeriod,
    q: &DtakoEventsQuery,
) -> Result<DtakoEventsAllResponse, ComputeError> {
    let page_size = q
        .page_size
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);

    let drivers = state
        .dtako_y_time_export
        .list_drivers_with_operations(
            tenant_id,
            q.date_from,
            q.date_to,
            q.after_driver_cd.as_deref(),
            page_size,
        )
        .await
        .map_err(|e| internal("list_drivers_with_operations", e))?;

    // 1 ページ分ちょうど返ったときだけ次ページがあり得る。
    let next_after_driver_cd = match drivers.len() as i64 == page_size {
        true => drivers.last().map(|d| d.driver_cd.clone()),
        false => None,
    };

    let driver_ids: Vec<Uuid> = drivers.iter().map(|d| d.driver_id).collect();
    let rows = state
        .dtako_y_time_export
        .list_operations_for_drivers(tenant_id, &driver_ids, q.date_from, q.date_to)
        .await
        .map_err(|e| internal("list_operations_for_drivers", e))?;

    // driver_id → ページ内の位置。運行行を乗務員ごとに振り分ける。
    let slot: HashMap<Uuid, usize> = driver_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, i))
        .collect();

    let mut plan = FetchPlan::default();
    let mut per_driver: Vec<Vec<PlannedOp>> =
        drivers.iter().map(|_| Vec::new()).collect::<Vec<_>>();
    for row in rows {
        // 直前のクエリを driver_ids で絞っているので通常は必ず引ける。
        // 引けない行 (repo 実装の取り違え等) は黙って捨てず飛ばすだけにする。
        if let Some(i) = slot.get(&row.driver_id).copied() {
            per_driver[i].push(PlannedOp::from_multi(tenant_id, row, &mut plan));
        }
    }

    // R2 fetch は key 単位で重複排除済みなので、同じ運行に相乗りした 2 名分でも 1 回で済む。
    let fetched = fetch_all(storage, plan.keys).await;

    let mut groups = Vec::with_capacity(drivers.len());
    let mut warnings = Vec::new();
    for (driver, planned) in drivers.into_iter().zip(per_driver) {
        let (operations, warns) = assemble(planned, &fetched);
        warnings.extend(warns);
        groups.push(DtakoEventsDriverGroup {
            driver: DtakoEventsDriver {
                cd: driver.driver_cd,
                name: driver.driver_name,
            },
            operations,
        });
    }
    // 同じ運行が複数乗務員に紐づくと同文の warning が重複するのでまとめる。
    warnings.sort();
    warnings.dedup();

    Ok(DtakoEventsAllResponse {
        period,
        drivers: groups,
        next_after_driver_cd,
        warnings,
    })
}

// =============================================================================
// GET /api/dtako/events/etags (Refs ohishi-exp/rust-ichibanboshi#205 実装計画 13)
// =============================================================================
//
// `GET /api/dtako/events` の sibling。CSV を一切ダウンロードせず、R2 の LIST 1 回
// (`StorageBackend::list`) だけで「テナントの当月分に変化があったか」の指紋を作る。
// `driver_cd` は無い — 呼び出し側 (rust-ichibanboshi の recalc) が欲しいのは
// 「その月まるごと」の指紋であって乗務員単位ではないため。
//
// 手順:
// 1. DB から期間内 (`collect_all_drivers` と同じ ±1 日広げ) の unko_no 集合を全乗務員分
//    1 回で列挙する。ページングは無し — `list_drivers_with_operations` に事実上無制限の
//    limit を渡して 1 クエリで全乗務員を取り、`list_operations_for_drivers` で運行を引く。
//    `MAX_RANGE_DAYS_ALL` (31 日) で範囲を縛っているので結果セットは月次の規模に収まる。
// 2. R2 に `{tenant_id}/unko/` prefix で LIST を 1 回投げる (`.download()` は一切呼ばない)。
// 3. LIST で返った key を `/KUDGIVT.csv` で終わるものだけに絞り、
//    `{tenant_id}/unko/{unko_no}/KUDGIVT.csv` の `{unko_no}` 部分を取り出す
//    (同じ unko_no 配下の KUDGURI.csv 等は無視する)。
// 4. DB 側の unko_no 集合と R2 側の unko_no→etag を突き合わせ、両方にある unko_no だけ
//    採用する。DB にはあるが R2 の LIST にまだ現れない unko_no (split がまだ終わっていない
//    等) は **etag: null で残す** (黙って落とすと「無かったこと」と区別が付かず、呼び出し側
//    はそのアップロードが安定するまで指紋に含めない、という判断ができなくなるため)。
//
// ETag が「内容が変わったか」の指紋として使える前提 (単一パート PUT の ETag = content MD5)
// については `alc_core::storage::ListedObject` の doc コメント参照。dtako CSV の upload 経路
// (`dtako_upload.rs`) はこの前提を満たす (`multipart: None` の単発 PUT のみ)。

/// 全乗務員版 `list_drivers_with_operations` を 1 ページで使い切るための limit。
/// この endpoint にページングの概念は無い (呼び出し側は月まるごとの指紋が欲しい) — 実際の
/// 結果セットは `MAX_RANGE_DAYS_ALL` (31 日) の DB クエリで既に月次規模に絞られている。
const ALL_DRIVERS_UNLIMITED: i64 = i64::MAX;

#[derive(Debug, Clone, Deserialize)]
pub struct DtakoEventsEtagsQuery {
    pub date_from: NaiveDate,
    pub date_to: NaiveDate,
}

/// 1 運行分の R2 指紋。`etag` が `null` は「DB にはあるが R2 の LIST にまだ現れていない」
/// ことを表す (アップロード直後の split 未完了など)。呼び出し側は `null` の unko_no を
/// 「まだ安定していない」として指紋のゲートに使わないことを想定している。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DtakoEventsEtagItem {
    pub unko_no: String,
    pub etag: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DtakoEventsEtagsResponse {
    pub period: DtakoEventsPeriod,
    pub items: Vec<DtakoEventsEtagItem>,
    pub warnings: Vec<String>,
}

async fn get_dtako_events_etags(
    State(state): State<DtakoState>,
    tenant: axum::Extension<TenantId>,
    Query(q): Query<DtakoEventsEtagsQuery>,
) -> Result<Json<DtakoEventsEtagsResponse>, (StatusCode, String)> {
    let tenant_id = tenant.0 .0;
    let resp = collect_dtako_events_etags(&state, tenant_id, q)
        .await
        .map_err(compute_error_to_response)?;
    Ok(Json(resp))
}

/// 共通 compute コア。handler と分離してテストしやすくしてある。
pub async fn collect_dtako_events_etags(
    state: &DtakoState,
    tenant_id: Uuid,
    q: DtakoEventsEtagsQuery,
) -> Result<DtakoEventsEtagsResponse, ComputeError> {
    validate_range(q.date_from, q.date_to, MAX_RANGE_DAYS_ETAGS)?;

    let storage: Arc<dyn StorageBackend> = state
        .dtako_storage
        .as_ref()
        .ok_or_else(|| ComputeError::Internal("dtako storage not configured".to_string()))?
        .clone();

    // 1. DB 側: 期間内に運行のある unko_no 集合を全乗務員 1 クエリで列挙する。
    let drivers = state
        .dtako_y_time_export
        .list_drivers_with_operations(
            tenant_id,
            q.date_from,
            q.date_to,
            None,
            ALL_DRIVERS_UNLIMITED,
        )
        .await
        .map_err(|e| internal("list_drivers_with_operations", e))?;

    let driver_ids: Vec<Uuid> = drivers.iter().map(|d| d.driver_id).collect();
    let rows = state
        .dtako_y_time_export
        .list_operations_for_drivers(tenant_id, &driver_ids, q.date_from, q.date_to)
        .await
        .map_err(|e| internal("list_operations_for_drivers", e))?;

    let mut db_unko_nos: Vec<String> = rows.into_iter().map(|r| r.unko_no).collect();
    db_unko_nos.sort();
    db_unko_nos.dedup();

    // 2. R2 側: LIST を「対象月」まで絞る。db_unko_nos の先頭 4 文字 (YYMM) が
    // distinct でだいたい 1〜2 個 (対象月 + 翌月またぎ 1 日ぶんの先読み) にしかならない
    // ことを使い、tenant 全体ではなく月ごとに LIST する (Refs
    // ohishi-exp/rust-ichibanboshi#205 comment 205-22)。db_unko_nos が空なら
    // 突き合わせる相手が無いので LIST を 1 回も呼ばない。
    let base_prefix = format!("{tenant_id}/unko/");
    let mut r2_etags: HashMap<String, String> = HashMap::new();
    if !db_unko_nos.is_empty() {
        let listed = match derive_yymm_prefixes(&db_unko_nos) {
            Some(yymms) => {
                let mut all = Vec::new();
                for yymm in yymms {
                    let prefix = format!("{base_prefix}{yymm}");
                    let page = storage.list(&prefix).await.map_err(|e| {
                        tracing::error!(prefix = %prefix, error = %e, "dtako-events-etags R2 list error");
                        ComputeError::Internal("internal error".to_string())
                    })?;
                    all.extend(page);
                }
                all
            }
            None => {
                // 安全弁: unko_no の先頭 4 文字が数字として取れない要素が 1 つでもあれば
                // 月で絞る前提が崩れるので、速さより正しさを優先して全 LIST に倒す。
                tracing::warn!(
                    "dtako-events-etags: unko_no から YYMM prefix を導けず全 LIST にフォールバック"
                );
                storage.list(&base_prefix).await.map_err(|e| {
                    tracing::error!(prefix = %base_prefix, error = %e, "dtako-events-etags R2 list error");
                    ComputeError::Internal("internal error".to_string())
                })?
            }
        };

        // 3. key → unko_no。`{base_prefix}{unko_no}/KUDGIVT.csv` の形のみ拾う。
        // LIST は要求した prefix に一致する key しか返さない (S3 系 API の保証) ので
        // `strip_prefix` 失敗は起こり得ないが、`unwrap_or_default` で 1 行に畳んで
        // 保険にしておく。KUDGURI.csv 等の兄弟ファイルは `strip_suffix` で弾く
        // (同じ unko_no ディレクトリに同居するため、prefix だけでは絞れない)。
        for obj in listed {
            let rest = obj.key.strip_prefix(&base_prefix).unwrap_or_default();
            let Some(unko_no) = rest.strip_suffix("/KUDGIVT.csv") else {
                continue;
            };
            if let Some(etag) = obj.etag {
                r2_etags.insert(unko_no.to_string(), etag);
            }
        }
    }

    // 4. 突き合わせ: DB にある unko_no だけを、R2 の etag (無ければ null) と組にして返す。
    let items = db_unko_nos
        .into_iter()
        .map(|unko_no| {
            let etag = r2_etags.get(&unko_no).cloned();
            DtakoEventsEtagItem { unko_no, etag }
        })
        .collect();

    Ok(DtakoEventsEtagsResponse {
        period: DtakoEventsPeriod {
            date_from: q.date_from,
            date_to: q.date_to,
        },
        items,
        warnings: Vec::new(),
    })
}

/// `db_unko_nos` の先頭 4 文字 (YYMM、`unko_no` の先頭 6 桁 `YYMMDD` の月部分) から
/// distinct な R2 LIST 用 prefix 候補を作る。1 件でも「先頭 4 文字が取れない / 数字で
/// ない」要素があれば `None` を返し、呼び出し側は安全側 (tenant 全体の LIST) に倒す。
///
/// 6 桁 (`YYMMDD`) ではなく 4 桁 (`YYMM`) を使うのは往復回数のため: 6 桁だと月あたり
/// 約 30 prefix (= LIST 30 往復) になるが、4 桁なら 1 か月ぶんの key が 1 prefix に
/// 収まり LIST は数ページで済む。
fn derive_yymm_prefixes(db_unko_nos: &[String]) -> Option<Vec<String>> {
    let mut yymms: Vec<&str> = Vec::with_capacity(db_unko_nos.len());
    for unko_no in db_unko_nos {
        let yymm = unko_no.get(0..4)?;
        if !yymm.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        yymms.push(yymm);
    }
    yymms.sort_unstable();
    yymms.dedup();
    Some(yymms.into_iter().map(str::to_string).collect())
}

/// R2 key の重複排除つき採番。同じ KUDGIVT.csv を 2 回落とさないため。
#[derive(Default)]
struct FetchPlan {
    keys: Vec<String>,
    index: HashMap<String, usize>,
}

impl FetchPlan {
    fn intern(&mut self, key: String) -> usize {
        match self.index.get(&key) {
            Some(i) => *i,
            None => {
                let i = self.keys.len();
                self.index.insert(key.clone(), i);
                self.keys.push(key);
                i
            }
        }
    }
}

/// fetch 待ちの 1 運行。`key_idx` は `FetchPlan::keys` への添字。
struct PlannedOp {
    unko_no: String,
    crew_role: i32,
    departure_at: Option<DateTime<Utc>>,
    return_at: Option<DateTime<Utc>>,
    key_idx: usize,
}

impl PlannedOp {
    fn from_single(tenant_id: Uuid, op: YTimeExportOperation, plan: &mut FetchPlan) -> Self {
        let key = build_kudgivt_key(tenant_id, &op.unko_no, op.r2_key_prefix.as_deref());
        Self {
            unko_no: op.unko_no,
            crew_role: op.crew_role,
            departure_at: op.departure_at,
            return_at: op.return_at,
            key_idx: plan.intern(key),
        }
    }

    fn from_multi(tenant_id: Uuid, op: DtakoDriverOperation, plan: &mut FetchPlan) -> Self {
        let key = build_kudgivt_key(tenant_id, &op.unko_no, op.r2_key_prefix.as_deref());
        Self {
            unko_no: op.unko_no,
            crew_role: op.crew_role,
            departure_at: op.departure_at,
            return_at: op.return_at,
            key_idx: plan.intern(key),
        }
    }
}

/// key を並列に落とす。戻り値は入力と同じ順序。
///
/// Cloud Run handler 内なので fire-and-forget な `tokio::spawn` は使わず、
/// `buffer_unordered` で await しきる。
async fn fetch_all(
    storage: &Arc<dyn StorageBackend>,
    keys: Vec<String>,
) -> Vec<Result<Vec<u8>, String>> {
    let started = std::time::Instant::now();
    let key_count = keys.len();

    let mut fetched: Vec<(usize, Result<Vec<u8>, String>)> =
        futures::stream::iter(keys.into_iter().enumerate().map(|(i, key)| {
            let storage = storage.clone();
            async move {
                let res = storage.download(&key).await.map_err(|e| e.to_string());
                (i, res)
            }
        }))
        .buffer_unordered(R2_FETCH_CONCURRENCY)
        .collect()
        .await;

    let ms = started.elapsed().as_millis();
    tracing::info!(keys = key_count, ms, "dtako-events R2 fetch");

    fetched.sort_by_key(|(i, _)| *i);
    fetched.into_iter().map(|(_, r)| r).collect()
}

/// fetch 結果を運行ごとの応答に組み立てる。落ちた運行は warning に落とす。
fn assemble(
    planned: Vec<PlannedOp>,
    fetched: &[Result<Vec<u8>, String>],
) -> (Vec<DtakoEventsOperation>, Vec<String>) {
    let mut operations = Vec::with_capacity(planned.len());
    let mut warnings = Vec::new();
    for p in planned {
        match &fetched[p.key_idx] {
            Ok(bytes) => {
                let (headers, rows) = parse_csv(&decode_csv_bytes(bytes));
                operations.push(DtakoEventsOperation {
                    unko_no: p.unko_no,
                    crew_role: p.crew_role,
                    departure_at: p.departure_at,
                    return_at: p.return_at,
                    headers,
                    rows,
                });
            }
            Err(e) => warnings.push(format!("{}: KUDGIVT 取得失敗 ({})", p.unko_no, e)),
        }
    }
    // buffer_unordered 由来のブレを消し、応答を決定的にする。
    operations.sort_by(|a, b| {
        a.departure_at
            .cmp(&b.departure_at)
            .then_with(|| a.unko_no.cmp(&b.unko_no))
    });
    warnings.sort();
    (operations, warnings)
}

/// DB エラーは呼び出し側に詳細を返さず loud log + 汎用 500 にする。
fn internal(op: &str, e: sqlx::Error) -> ComputeError {
    tracing::error!(op, error = %e, "dtako-events db error");
    ComputeError::Internal("internal error".to_string())
}

/// 期間の妥当性検査。逆転と上限超過を 400 で弾く。
fn validate_range(
    date_from: NaiveDate,
    date_to: NaiveDate,
    max_days: i64,
) -> Result<(), ComputeError> {
    if date_from > date_to {
        return Err(ComputeError::BadRequest("date_from > date_to".to_string()));
    }
    let span_days = (date_to - date_from).num_days() + 1;
    if span_days > max_days {
        return Err(ComputeError::BadRequest(format!(
            "range too wide: {span_days} days (max {max_days})"
        )));
    }
    Ok(())
}

/// ページングパラメータは全乗務員版専用。単一乗務員版に付いていたら黙って無視せず 400 で返す。
fn reject_paging_params(
    page_size: Option<i64>,
    after_driver_cd: Option<&str>,
) -> Result<(), ComputeError> {
    if page_size.is_some() || after_driver_cd.is_some() {
        return Err(ComputeError::BadRequest(
            "page_size / after_driver_cd are only valid without driver_cd".to_string(),
        ));
    }
    Ok(())
}

/// R2 の per-unko CSV は split 時に UTF-8 化済み。古い Shift-JIS データのために
/// フォールバックを残す (`csv_aggregator` と同じ判断)。
fn decode_csv_bytes(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => decode_shift_jis(bytes),
    }
}

/// CSV → `(headers, rows)`。`dtako_csv_proxy` と同じ素朴な split
/// (KUDGIVT はクォート無しの固定形式)。
fn parse_csv(text: &str) -> (Vec<String>, Vec<Vec<String>>) {
    let mut lines = text.lines();
    let headers: Vec<String> = lines
        .next()
        .unwrap_or("")
        .split(',')
        .map(|h| h.trim().to_string())
        .collect();
    let rows: Vec<Vec<String>> = lines
        .filter(|l| !l.trim().is_empty())
        .map(|line| line.split(',').map(|f| f.trim().to_string()).collect())
        .collect();
    (headers, rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn validate_range_accepts_single_day() {
        assert!(validate_range(d(2026, 6, 1), d(2026, 6, 1), MAX_RANGE_DAYS_SINGLE).is_ok());
    }

    #[test]
    fn validate_range_accepts_one_calendar_month_for_all_drivers() {
        // 31 日の月がちょうど通る = 全乗務員版で暦月を 1 回で引ける
        assert!(validate_range(d(2026, 7, 1), d(2026, 7, 31), MAX_RANGE_DAYS_ALL).is_ok());
    }

    #[test]
    fn validate_range_rejects_reversed() {
        let err = validate_range(d(2026, 6, 30), d(2026, 6, 1), MAX_RANGE_DAYS_SINGLE).unwrap_err();
        assert!(matches!(err, ComputeError::BadRequest(_)));
        assert_eq!(err.to_string(), "date_from > date_to");
    }

    #[test]
    fn validate_range_accepts_exactly_max() {
        // 2026-01-01 〜 2027-01-01 は 366 日 (境界ちょうど)
        assert!(validate_range(d(2026, 1, 1), d(2027, 1, 1), MAX_RANGE_DAYS_SINGLE).is_ok());
    }

    #[test]
    fn validate_range_rejects_over_max() {
        let err = validate_range(d(2026, 1, 1), d(2027, 1, 2), MAX_RANGE_DAYS_SINGLE).unwrap_err();
        assert!(err.to_string().contains("range too wide"));
        assert!(err.to_string().contains("367"));
    }

    #[test]
    fn validate_range_all_drivers_limit_is_much_tighter() {
        // 全乗務員で 32 日は弾かれる (R2 GET 約 1,128 回 / 月が上限の根拠)
        let err = validate_range(d(2026, 6, 1), d(2026, 7, 2), MAX_RANGE_DAYS_ALL).unwrap_err();
        assert!(err.to_string().contains("max 31"));
    }

    #[test]
    fn reject_paging_params_allows_none() {
        assert!(reject_paging_params(None, None).is_ok());
    }

    #[test]
    fn reject_paging_params_rejects_page_size() {
        assert!(reject_paging_params(Some(10), None).is_err());
    }

    #[test]
    fn reject_paging_params_rejects_cursor() {
        let err = reject_paging_params(None, Some("D001")).unwrap_err();
        assert!(err.to_string().contains("only valid without driver_cd"));
    }

    #[test]
    fn fetch_plan_dedups_identical_keys() {
        let mut plan = FetchPlan::default();
        assert_eq!(plan.intern("a".into()), 0);
        assert_eq!(plan.intern("b".into()), 1);
        assert_eq!(plan.intern("a".into()), 0);
        assert_eq!(plan.keys, vec!["a", "b"]);
    }

    #[test]
    fn parse_csv_splits_header_and_rows_and_skips_blank_lines() {
        let (headers, rows) = parse_csv("a, b ,c\n1,2,3\n\n4,5,6\n");
        assert_eq!(headers, vec!["a", "b", "c"]);
        assert_eq!(rows, vec![vec!["1", "2", "3"], vec!["4", "5", "6"]]);
    }

    #[test]
    fn parse_csv_on_empty_input_yields_single_empty_header() {
        let (headers, rows) = parse_csv("");
        assert_eq!(headers, vec![""]);
        assert!(rows.is_empty());
    }

    #[test]
    fn decode_csv_bytes_reads_utf8_as_is() {
        assert_eq!(
            decode_csv_bytes("運行NO,読取日".as_bytes()),
            "運行NO,読取日"
        );
    }

    #[test]
    fn decode_csv_bytes_falls_back_to_shift_jis() {
        // "運行NO,読取日" の Shift-JIS bytes。UTF-8 として不正なので、一致するのは
        // フォールバックが走った場合だけ。encoding_rs を alc-dtako の dev-dependency に
        // 足さずに済ませるため直書きする。
        let sjis: &[u8] = &[
            0x89, 0x5e, 0x8d, 0x73, 0x4e, 0x4f, 0x2c, 0x93, 0xc7, 0x8e, 0xe6, 0x93, 0xfa,
        ];
        assert_eq!(decode_csv_bytes(sjis), "運行NO,読取日");
    }

    fn planned(unko_no: &str, key_idx: usize) -> PlannedOp {
        PlannedOp {
            unko_no: unko_no.to_string(),
            crew_role: 1,
            departure_at: None,
            return_at: None,
            key_idx,
        }
    }

    const CSV: &str = "運行NO,対象乗務員区分,イベントCD\nU1,1,201\nU1,2,201\n";

    #[test]
    fn assemble_returns_every_row_verbatim() {
        // 副運転手 (対象乗務員区分 = 2) の行も落とさない。絞るのは呼び出し側の仕事。
        let fetched = vec![Ok(CSV.as_bytes().to_vec())];
        let (ops, warns) = assemble(vec![planned("U1", 0)], &fetched);
        assert!(warns.is_empty());
        assert_eq!(ops.len(), 1);
        assert_eq!(
            ops[0].headers,
            vec!["運行NO", "対象乗務員区分", "イベントCD"]
        );
        assert_eq!(
            ops[0].rows,
            vec![vec!["U1", "1", "201"], vec!["U1", "2", "201"]]
        );
        assert_eq!(ops[0].crew_role, 1);
    }

    #[test]
    fn assemble_maps_download_error_to_warning() {
        let fetched = vec![Err("NoSuchKey".to_string())];
        let (ops, warns) = assemble(vec![planned("U1", 0)], &fetched);
        assert!(ops.is_empty());
        assert_eq!(warns.len(), 1);
        assert!(warns[0].contains("U1"));
        assert!(warns[0].contains("NoSuchKey"));
    }

    #[test]
    fn assemble_shares_one_fetch_between_two_operations() {
        // 同じ運行に 2 名が相乗り = 同じ key_idx。R2 GET は 1 回で両方に配られる。
        let fetched = vec![Ok(CSV.as_bytes().to_vec())];
        let (ops, _) = assemble(vec![planned("U1", 0), planned("U1", 0)], &fetched);
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0].rows, ops[1].rows);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn concurrency_and_page_size_are_in_safe_range() {
        assert!(R2_FETCH_CONCURRENCY >= 4);
        assert!(R2_FETCH_CONCURRENCY <= 64);
        // 1 ページ 50 名 ≒ 8 MB。Cloud Run の応答上限 32 MiB に収まること
        assert!(DEFAULT_PAGE_SIZE <= MAX_PAGE_SIZE);
        assert!(MAX_PAGE_SIZE <= 50);
        // 全乗務員版は単一乗務員版より必ず狭い
        assert!(MAX_RANGE_DAYS_ALL < MAX_RANGE_DAYS_SINGLE);
    }

    #[test]
    fn etags_range_cap_covers_month_range_with_next_month_lookahead() {
        // rust-ichibanboshi 側の month_range(month) は暦月 (最大 31 日) + 翌月またぎ
        // 猶予 1 日 = 最大 32 日。R2 GET を伴わない LIST 専用 endpoint なので
        // MAX_RANGE_DAYS_ALL (CSV ダウンロード枚数根拠) より広く取ってよい。
        assert!(MAX_RANGE_DAYS_ETAGS >= 32);
        assert!(validate_range(d(2026, 7, 1), d(2026, 8, 1), MAX_RANGE_DAYS_ETAGS).is_ok());
    }
}

//! 運行 (dtako_operations) の上げ直し・手動削除の変更記録
//! (Refs ohishi-exp/nuxt-dtako-admin#1133 訴訟用の準備ページ)。
//!
//! 同じ運行を上げ直すと dtako_operations の行は DELETE → INSERT されるので、前の値が
//! どこにも残らない。上げ直し (`process_zip`) と手動削除 (`DELETE /operations/{unko_no}`)
//! の 2 つの口で、消す直前の値を `dtako_operation_changes` に残す。
//! ここには読み口と、R2 の旧 KUDGIVT の読み込みを置く。SQL は repo 層 (`repo::dtako_operation_changes`)、
//! 分数の計算・snapshot の組み立て・比較 (純粋な部分) は `alc_csv_parser::operation_changes` (分割 worker と共有)。
//!
//! 休憩などの分数は dtako_operations の列に無い (`op_*` 列は一度も書かれていない) ので、
//! KUDGIVT の区間時間から出す。上げ直しの before は split 済みの R2 旧 KUDGIVT
//! (= GCP が読んでいた値)、after は今回の zip の KUDGIVT。

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::dtako_y_time_export::csv_aggregator::fetch_and_parse_kudgivt;
use crate::DtakoState;
use alc_core::auth_middleware::TenantId;
use alc_core::repository::dtako_operations::OperationChangeRow;
use alc_core::repository::dtako_upload::OperationMinutes;
use alc_core::storage::StorageBackend;
use alc_csv_parser::operation_changes::minutes_from_events;

pub fn tenant_router<S>() -> Router<S>
where
    DtakoState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new().route("/dtako/operation-changes", get(list_operation_changes))
}

/// split 済みの R2 旧 KUDGIVT から 1 運行・1 crew_role ぶんの分数を出す。
/// 上げ直しの split は `process_zip` の後に走るので、この時点の R2 は前回の値のまま。
/// 取れなければ `None` (前回の split 失敗など) — 分数は比較から外す。
pub async fn load_before_minutes(
    storage: &dyn StorageBackend,
    tenant_id: Uuid,
    unko_no: &str,
    crew_role: i32,
) -> Option<OperationMinutes> {
    match fetch_and_parse_kudgivt(storage, tenant_id, unko_no, None, crew_role).await {
        Ok(rows) => Some(minutes_from_events(&rows)),
        Err(e) => {
            tracing::warn!("operation change: old KUDGIVT unavailable {unko_no}: {e}");
            None
        }
    }
}

/// 運行の日付。unko_no の先頭 6 桁 (YYMMDD) を正とし、読めなければ
/// before/after の departure_at (DB に壁時計のまま UTC で入っている) の日付。
pub fn operation_date_of(row: &OperationChangeRow) -> Option<NaiveDate> {
    let from_unko = row
        .unko_no
        .get(..6)
        .and_then(|p| NaiveDate::parse_from_str(&format!("20{p}"), "%Y%m%d").ok());
    let from_departure = || {
        [row.before.as_ref(), row.after.as_ref()]
            .into_iter()
            .flatten()
            .filter_map(|v| v.get("departure_at").and_then(|d| d.as_str()))
            .find_map(|d| NaiveDate::parse_from_str(d.get(..10)?, "%Y-%m-%d").ok())
    };
    from_unko.or_else(from_departure)
}

#[derive(Debug, Deserialize)]
pub struct OperationChangesQuery {
    pub driver_cd: String,
    pub from: NaiveDate,
    pub to: NaiveDate,
}

#[derive(Debug, Serialize)]
pub struct OperationChangesResponse {
    pub driver_cd: String,
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// 記録を始めた時点 (表の最古 recorded_at)。これより前の変更は記録に無い
    pub recording_since: Option<DateTime<Utc>>,
    pub changes: Vec<OperationChangeRow>,
}

/// この乗務員のこの期間の運行が、取り込み後に変わったか。期間は recorded_at ではなく
/// 運行の日付で絞る。
async fn list_operation_changes(
    State(state): State<DtakoState>,
    tenant: axum::Extension<TenantId>,
    Query(q): Query<OperationChangesQuery>,
) -> Result<Json<OperationChangesResponse>, StatusCode> {
    if q.from > q.to {
        return Err(StatusCode::BAD_REQUEST);
    }
    let tenant_id = tenant.0 .0;
    let rows = state
        .dtako_operations
        .list_operation_changes(tenant_id, &q.driver_cd)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let recording_since = state
        .dtako_operations
        .operation_changes_recording_since(tenant_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let changes = rows
        .into_iter()
        .filter(|r| operation_date_of(r).is_some_and(|d| d >= q.from && d <= q.to))
        .collect();
    Ok(Json(OperationChangesResponse {
        driver_cd: q.driver_cd,
        from: q.from,
        to: q.to,
        recording_since,
        changes,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(unko_no: &str, before: Option<serde_json::Value>) -> OperationChangeRow {
        OperationChangeRow {
            unko_no: unko_no.to_string(),
            crew_role: 1,
            recorded_at: Utc::now(),
            reason: "reupload".to_string(),
            before,
            after: None,
        }
    }

    #[test]
    fn test_operation_date_of() {
        test_group!("運行の変更記録");
        test_case!(
            "unko_no の先頭 6 桁、読めなければ departure_at",
            {
                assert_eq!(
                    operation_date_of(&row("2605230341010000004219", None)),
                    NaiveDate::from_ymd_opt(2026, 5, 23)
                );
                let dep = json!({"departure_at": "2026-05-22T23:10:00Z"});
                assert_eq!(
                    operation_date_of(&row("X1", Some(dep))),
                    NaiveDate::from_ymd_opt(2026, 5, 22)
                );
                let mut after_only = row("ABCDEF123", None);
                after_only.after = Some(json!({"departure_at": "2026-06-01T01:00:00Z"}));
                assert_eq!(
                    operation_date_of(&after_only),
                    NaiveDate::from_ymd_opt(2026, 6, 1)
                );
                let broken = json!({"departure_at": "bad"});
                assert_eq!(operation_date_of(&row("X1", Some(broken))), None);
                assert_eq!(operation_date_of(&row("X1", None)), None);
            }
        );
    }
}

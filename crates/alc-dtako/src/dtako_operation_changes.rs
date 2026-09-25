//! 運行 (dtako_operations) の上げ直し・手動削除の変更記録
//! (Refs ohishi-exp/nuxt-dtako-admin#1133 訴訟用の準備ページ)。
//!
//! 同じ運行を上げ直すと dtako_operations の行は DELETE → INSERT されるので、前の値が
//! どこにも残らない。上げ直し (`process_zip`) と手動削除 (`DELETE /operations/{unko_no}`)
//! の 2 つの口で、消す直前の値を `dtako_operation_changes` に残す。
//! ここには比較と読み口を置き、SQL は repo 層 (`repo::dtako_operation_changes`) に置く。
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

use crate::dtako_upload::default_classification;
use crate::dtako_y_time_export::csv_aggregator::fetch_and_parse_kudgivt;
use crate::DtakoState;
use alc_core::auth_middleware::TenantId;
use alc_core::repository::dtako_operations::OperationChangeRow;
use alc_core::repository::dtako_upload::OperationMinutes;
use alc_core::storage::StorageBackend;
use alc_csv_parser::kudgivt::KudgivtRow;
use alc_csv_parser::work_segments::EventClass;

pub fn tenant_router<S>() -> Router<S>
where
    DtakoState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new().route("/dtako/operation-changes", get(list_operation_changes))
}

/// KUDGIVT の区間時間をイベントCD の既定分類で合計する。
/// tenant ごとの分類 (dtako_event_classifications) は使わない — 記録の意味が
/// 分類の編集で後から変わらないようにするため。
pub fn minutes_from_events<'a>(
    events: impl IntoIterator<Item = &'a KudgivtRow>,
) -> OperationMinutes {
    let mut m = OperationMinutes::default();
    for e in events {
        let dur = e.duration_minutes.unwrap_or(0);
        match default_classification(&e.event_cd).1 {
            EventClass::Drive => m.drive_minutes += dur,
            EventClass::Cargo => m.cargo_minutes += dur,
            EventClass::Break => m.break_minutes += dur,
            EventClass::RestSplit => m.rest_minutes += dur,
            EventClass::Ignore => {}
        }
    }
    m
}

/// 今回の zip の KUDGIVT から 1 運行・1 crew_role ぶんの分数を出す。
/// crew_role で絞るのは、2マンの運行で運転手と助手のイベントを取り違えないため。
pub fn minutes_for(events: &[KudgivtRow], unko_no: &str, crew_role: i32) -> OperationMinutes {
    minutes_from_events(
        events
            .iter()
            .filter(|e| e.unko_no == unko_no && e.crew_role == crew_role),
    )
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

/// before の旧 KUDGIVT が取れなかった回に before へ残す印のキー。値は `"unavailable"`。
/// 黙って「分数の変化なし」に見せないための印で、比較 (`snapshot_changed`) からは外す。
pub const BEFORE_KUDGIVT_KEY: &str = "before_kudgivt";

/// DB から読んだ `{driver_cd, departure_at, return_at}` に分数を足す。
/// `minutes` が `None` なのは before の旧 KUDGIVT が取れなかったときだけ (after は常に zip
/// から出る) で、そのときは分数の代わりに `before_kudgivt: "unavailable"` を残す。
pub fn compose_snapshot(
    mut db: serde_json::Value,
    minutes: Option<&OperationMinutes>,
) -> serde_json::Value {
    if let Some(obj) = db.as_object_mut() {
        match minutes {
            Some(m) => {
                obj.insert("drive_minutes".into(), m.drive_minutes.into());
                obj.insert("cargo_minutes".into(), m.cargo_minutes.into());
                obj.insert("break_minutes".into(), m.break_minutes.into());
                obj.insert("rest_minutes".into(), m.rest_minutes.into());
            }
            None => {
                obj.insert(BEFORE_KUDGIVT_KEY.into(), "unavailable".into());
            }
        }
    }
    db
}

/// before に在るキーのうち、after で値が違うものが 1 つでもあれば変わったとみなす。
/// before に無いキー (旧 KUDGIVT が取れなかった分数) と印 (`before_kudgivt`) は比べない。
pub fn snapshot_changed(before: &serde_json::Value, after: &serde_json::Value) -> bool {
    match before.as_object() {
        Some(b) => b
            .iter()
            .filter(|(k, _)| k.as_str() != BEFORE_KUDGIVT_KEY)
            .any(|(k, v)| after.get(k) != Some(v)),
        None => before != after,
    }
}

/// 記録の driver_cd 列に入れる値。after を優先し、無ければ before。
pub fn record_driver_cd(
    before: Option<&serde_json::Value>,
    after: Option<&serde_json::Value>,
) -> Option<String> {
    let cd = |v: Option<&serde_json::Value>| {
        v.and_then(|v| v.get("driver_cd"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
    };
    cd(after).or_else(|| cd(before))
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

    fn evt(unko_no: &str, crew_role: i32, event_cd: &str, dur: Option<i32>) -> KudgivtRow {
        KudgivtRow {
            unko_no: unko_no.to_string(),
            reading_date: NaiveDate::from_ymd_opt(2026, 5, 24).unwrap(),
            driver_cd: "1194".to_string(),
            driver_name: String::new(),
            crew_role,
            start_at: NaiveDate::from_ymd_opt(2026, 5, 23)
                .unwrap()
                .and_hms_opt(3, 41, 1)
                .unwrap(),
            end_at: None,
            event_cd: event_cd.to_string(),
            event_name: String::new(),
            duration_minutes: dur,
            section_distance: None,
            raw_data: json!({}),
        }
    }

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
    fn test_minutes_from_events_classifies_by_event_cd() {
        test_group!("運行の変更記録");
        test_case!(
            "イベントCD の既定分類で振り分けて合計する",
            {
                let events = vec![
                    evt("U", 1, "201", Some(100)),
                    evt("U", 1, "202", Some(10)),
                    evt("U", 1, "203", Some(5)),
                    evt("U", 1, "301", Some(120)),
                    evt("U", 1, "301", Some(58)),
                    evt("U", 1, "302", Some(480)),
                    evt("U", 1, "101", Some(999)),
                    evt("U", 1, "301", None),
                ];
                let m = minutes_from_events(&events);
                assert_eq!(
                    m,
                    OperationMinutes {
                        drive_minutes: 100,
                        cargo_minutes: 15,
                        break_minutes: 178,
                        rest_minutes: 480,
                    }
                );
            }
        );
    }

    #[test]
    fn test_minutes_for_filters_unko_and_crew_role() {
        test_group!("運行の変更記録");
        test_case!("2マンの運行で crew_role ごとに分ける", {
            let events = vec![
                evt("U", 1, "301", Some(30)),
                evt("U", 2, "301", Some(178)),
                evt("OTHER", 1, "301", Some(999)),
            ];
            assert_eq!(minutes_for(&events, "U", 1).break_minutes, 30);
            assert_eq!(minutes_for(&events, "U", 2).break_minutes, 178);
            assert_eq!(minutes_for(&events, "U", 3), OperationMinutes::default());
        });
    }

    #[test]
    fn test_compose_snapshot() {
        test_group!("運行の変更記録");
        test_case!(
            "分数があればキーを足し、無ければ DB の値のまま",
            {
                let db = json!({"driver_cd": "1194", "departure_at": null, "return_at": null});
                let m = OperationMinutes {
                    break_minutes: 178,
                    ..Default::default()
                };
                let with = compose_snapshot(db.clone(), Some(&m));
                assert_eq!(with["break_minutes"], 178);
                assert_eq!(with["drive_minutes"], 0);
                assert_eq!(with["cargo_minutes"], 0);
                assert_eq!(with["rest_minutes"], 0);
                // 旧 KUDGIVT が取れなかった before は分数の代わりに印を残す
                let without = compose_snapshot(db.clone(), None);
                assert_eq!(without["before_kudgivt"], "unavailable");
                assert!(without.get("break_minutes").is_none());
                // object でなければ触らない
                assert_eq!(compose_snapshot(json!(null), Some(&m)), json!(null));
            }
        );
    }

    #[test]
    fn test_snapshot_changed() {
        test_group!("運行の変更記録");
        test_case!("before のキーだけを比べる", {
            let before = json!({"driver_cd": "1194", "break_minutes": 0});
            let same = json!({"driver_cd": "1194", "break_minutes": 0, "rest_minutes": 5});
            let diff = json!({"driver_cd": "1194", "break_minutes": 178});
            assert!(!snapshot_changed(&before, &same));
            assert!(snapshot_changed(&before, &diff));
            // before の分数が取れなかった回は分数を比べない
            let before_no_minutes = json!({"driver_cd": "1194", "before_kudgivt": "unavailable"});
            assert!(!snapshot_changed(&before_no_minutes, &diff));
            let other_driver = json!({"driver_cd": "1500", "break_minutes": 178});
            assert!(snapshot_changed(&before_no_minutes, &other_driver));
            // after にキーが無ければ変わったとみなす
            assert!(snapshot_changed(&before, &json!({"driver_cd": "1194"})));
            // object でなければ全体で比べる
            assert!(!snapshot_changed(&json!(null), &json!(null)));
            assert!(snapshot_changed(&json!(null), &diff));
        });
    }

    #[test]
    fn test_record_driver_cd() {
        test_group!("運行の変更記録");
        test_case!("after を優先し、無ければ before", {
            let a = json!({"driver_cd": "2"});
            let b = json!({"driver_cd": "1"});
            let null_cd = json!({"driver_cd": null});
            assert_eq!(record_driver_cd(Some(&b), Some(&a)).as_deref(), Some("2"));
            assert_eq!(record_driver_cd(Some(&b), None).as_deref(), Some("1"));
            assert_eq!(
                record_driver_cd(Some(&b), Some(&null_cd)).as_deref(),
                Some("1")
            );
            assert_eq!(record_driver_cd(None, None), None);
        });
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

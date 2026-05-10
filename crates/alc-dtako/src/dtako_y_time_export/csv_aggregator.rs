//! KUDGIVT.csv (R2) → SegmentInput への変換。
//!
//! 既存の `alc_csv_parser` を流用し、本機能のためだけに新しい parsing は導入しない。

use alc_core::storage::{StorageBackend, StorageError};
use alc_csv_parser::decode_shift_jis;
use alc_csv_parser::kudgivt::{parse_kudgivt, KudgivtRow};
use alc_csv_parser::work_segments::{split_by_rest, EventClass, WorkSegment};
use chrono::NaiveDateTime;
use std::collections::HashMap;

use super::builder::SegmentInput;

/// イベント分類マップを default_classification 相当で組み立てる。
///
/// `dtako_upload::default_classification` と同じマッピングを inline 化することで、
/// この read-only エンドポイントは tenant の event_classifications テーブルに対する
/// upsert 副作用を起こさない。tenant が分類を上書きしている場合 (例: 110 を Drive に
/// 再分類) には対応しないが、Phase 1 のスコープでは default のみで充分。
fn classify(event_cd: &str) -> EventClass {
    match event_cd {
        "201" => EventClass::Drive,
        "202" | "203" | "204" => EventClass::Cargo,
        "302" => EventClass::RestSplit,
        "301" => EventClass::Break,
        _ => EventClass::Ignore,
    }
}

/// 1 unko_no 分の処理エラー。`thiserror` を新たに依存追加せず手動 impl。
#[derive(Debug)]
pub enum AggregatorError {
    Storage(StorageError),
    Parse(String),
    MissingTimes(String),
}

impl std::fmt::Display for AggregatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Storage(e) => write!(f, "storage error: {e}"),
            Self::Parse(e) => write!(f, "csv parse error: {e}"),
            Self::MissingTimes(unko) => {
                write!(f, "missing departure_at / return_at on operation {unko}")
            }
        }
    }
}

impl std::error::Error for AggregatorError {}

impl From<StorageError> for AggregatorError {
    fn from(e: StorageError) -> Self {
        Self::Storage(e)
    }
}

/// R2 path 解決:
/// - `r2_key_prefix` が None なら `{tenant_id}/unko/{unko_no}/KUDGIVT.csv` (既存 fallback)
pub fn build_kudgivt_key(tenant_id: uuid::Uuid, unko_no: &str, r2_prefix: Option<&str>) -> String {
    match r2_prefix {
        Some(p) => format!("{}/KUDGIVT.csv", p.trim_end_matches('/')),
        None => format!("{}/unko/{}/KUDGIVT.csv", tenant_id, unko_no),
    }
}

/// 1 運行の KUDGIVT.csv を取得して該当 crew_role の events を返す。
///
/// `split_csv_from_r2` (dtako_upload.rs) で per-unko CSV は **UTF-8 で保存**される
/// (元 ZIP 内が Shift-JIS でも、split 時に decode_shift_jis を通して UTF-8 化済み)。
/// なので UTF-8 として読む。後方互換のため Shift-JIS もフォールバックで試す
/// (元 R2 に Shift-JIS のまま置かれた古いデータ用)。
pub async fn fetch_and_parse_kudgivt(
    storage: &dyn StorageBackend,
    tenant_id: uuid::Uuid,
    unko_no: &str,
    r2_prefix: Option<&str>,
    crew_role: i32,
) -> Result<Vec<KudgivtRow>, AggregatorError> {
    let key = build_kudgivt_key(tenant_id, unko_no, r2_prefix);
    let bytes = storage.download(&key).await?;
    // まず UTF-8 として試す。失敗したら (= 古い Shift-JIS データ) decode_shift_jis にフォールバック
    let text = match std::str::from_utf8(&bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => decode_shift_jis(&bytes),
    };
    let rows = parse_kudgivt(&text).map_err(|e| AggregatorError::Parse(e.to_string()))?;
    Ok(rows
        .into_iter()
        .filter(|r| r.crew_role == crew_role)
        .collect())
}

/// 1 運行 (KUDGIVT events + departure/return) を `Vec<SegmentInput>` に変換する。
///
/// - `split_by_rest` で WorkSegment を出す
/// - 各 segment 内の 301 events を sum し rest_minutes として付与
/// - 1 segment が 24h を超えた場合 (= 休息イベント無しで 24h+ 連続労働、改善基準告示違反データ) は
///   **24h で強制 cut**。各 sub-segment の rest_minutes は該当範囲の 301 events を sum しなおす。
///   これにより builder 側 ((A) bucketing) が前日/当日/翌日 (3 暦日) の範囲で対応できる
pub fn build_segment_inputs(
    events: &[KudgivtRow],
    departure_at: NaiveDateTime,
    return_at: NaiveDateTime,
) -> Vec<SegmentInput> {
    let classifications: HashMap<String, EventClass> = events
        .iter()
        .map(|e| (e.event_cd.clone(), classify(&e.event_cd)))
        .collect();

    let event_refs: Vec<&KudgivtRow> = events.iter().collect();
    let segments: Vec<WorkSegment> =
        split_by_rest(departure_at, return_at, &event_refs, &classifications);

    segments
        .into_iter()
        .flat_map(|s| split_at_24h(events, s.start, s.end))
        .collect()
}

/// 1 WorkSegment を 24h 境界で分割。`end - start <= 24h` ならそのまま 1 個。
fn split_at_24h(
    events: &[KudgivtRow],
    start: NaiveDateTime,
    end: NaiveDateTime,
) -> Vec<SegmentInput> {
    let mut out = Vec::new();
    let mut cur = start;
    while cur < end {
        let chunk_end = std::cmp::min(cur + chrono::Duration::hours(24), end);
        let intervals = collect_break_intervals(events, cur, chunk_end);
        let rest_minutes = intervals
            .iter()
            .map(|(s, e)| (*e - *s).num_minutes() as i32)
            .sum();
        out.push(SegmentInput {
            start: cur,
            end: chunk_end,
            rest_minutes,
            rest_intervals: intervals,
            note: None,
        });
        cur = chunk_end;
    }
    out
}

/// `[seg_start, seg_end]` 内の event_cd=301 の (start, end) を集める。
/// duration_minutes が None の場合はその event は無視 (期間 0 とみなす)。
fn collect_break_intervals(
    events: &[KudgivtRow],
    seg_start: NaiveDateTime,
    seg_end: NaiveDateTime,
) -> Vec<(NaiveDateTime, NaiveDateTime)> {
    events
        .iter()
        .filter(|e| e.event_cd == "301")
        .filter(|e| e.start_at >= seg_start && e.start_at <= seg_end)
        .filter_map(|e| {
            let dur = e.duration_minutes?;
            if dur <= 0 {
                return None;
            }
            let s = e.start_at;
            let raw_end = s + chrono::Duration::minutes(dur as i64);
            // segment 末尾でクリップ (segment 跨ぎ休憩は途中で切る)
            let e_clip = raw_end.min(seg_end);
            if e_clip <= s {
                None
            } else {
                Some((s, e_clip))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn dt(date: (i32, u32, u32), time: (u32, u32, u32)) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(date.0, date.1, date.2)
            .unwrap()
            .and_hms_opt(time.0, time.1, time.2)
            .unwrap()
    }

    fn ev(event_cd: &str, start: NaiveDateTime, dur: Option<i32>) -> KudgivtRow {
        KudgivtRow {
            unko_no: "UN1".into(),
            reading_date: NaiveDate::from_ymd_opt(2024, 4, 15).unwrap(),
            driver_cd: "D1".into(),
            driver_name: "Test".into(),
            crew_role: 1,
            start_at: start,
            end_at: dur.map(|d| start + chrono::Duration::minutes(d as i64)),
            event_cd: event_cd.into(),
            event_name: "x".into(),
            duration_minutes: dur,
            section_distance: None,
            raw_data: serde_json::json!({}),
        }
    }

    #[test]
    fn key_uses_r2_prefix_when_provided() {
        let tid = uuid::Uuid::nil();
        let k = build_kudgivt_key(tid, "u1", Some("custom/prefix/u1"));
        assert_eq!(k, "custom/prefix/u1/KUDGIVT.csv");
    }

    #[test]
    fn key_uses_fallback_path_when_no_prefix() {
        let tid = uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let k = build_kudgivt_key(tid, "u1", None);
        assert_eq!(
            k,
            "11111111-1111-1111-1111-111111111111/unko/u1/KUDGIVT.csv"
        );
    }

    #[test]
    fn classify_default_mapping_covers_known_codes() {
        assert_eq!(classify("201"), EventClass::Drive);
        assert_eq!(classify("202"), EventClass::Cargo);
        assert_eq!(classify("203"), EventClass::Cargo);
        assert_eq!(classify("204"), EventClass::Cargo);
        assert_eq!(classify("301"), EventClass::Break);
        assert_eq!(classify("302"), EventClass::RestSplit);
        assert_eq!(classify("999"), EventClass::Ignore);
    }

    #[test]
    fn collect_break_intervals_filters_by_event_cd_and_range() {
        let events = vec![
            ev("301", dt((2024, 4, 15), (10, 0, 0)), Some(30)),
            ev("301", dt((2024, 4, 15), (14, 0, 0)), Some(15)),
            ev("301", dt((2024, 4, 15), (20, 0, 0)), Some(60)), // 範囲外
            ev("302", dt((2024, 4, 15), (12, 0, 0)), Some(540)), // event_cd 違い
        ];
        let intervals = collect_break_intervals(
            &events,
            dt((2024, 4, 15), (8, 0, 0)),
            dt((2024, 4, 15), (18, 0, 0)),
        );
        assert_eq!(intervals.len(), 2);
        let total: i32 = intervals
            .iter()
            .map(|(s, e)| (*e - *s).num_minutes() as i32)
            .sum();
        assert_eq!(total, 30 + 15);
    }

    #[test]
    fn build_segment_inputs_with_no_rest_split_returns_one_segment() {
        // 8:00 〜 17:00、間に 301 が 60 分
        let events = vec![
            ev("201", dt((2024, 4, 15), (8, 0, 0)), Some(120)),
            ev("301", dt((2024, 4, 15), (12, 0, 0)), Some(60)),
            ev("201", dt((2024, 4, 15), (13, 0, 0)), Some(240)),
        ];
        let segs = build_segment_inputs(
            &events,
            dt((2024, 4, 15), (8, 0, 0)),
            dt((2024, 4, 15), (17, 0, 0)),
        );
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].rest_minutes, 60);
        assert_eq!(segs[0].start, dt((2024, 4, 15), (8, 0, 0)));
    }

    #[test]
    fn build_segment_inputs_splits_at_long_rest() {
        // 4/15 22:00 〜 4/16 09:00 だが、間に 302 (休息 600 分) が 4/15 23:00〜4/16 09:00
        let events = vec![
            ev("201", dt((2024, 4, 15), (22, 0, 0)), Some(60)),
            ev(
                "302",
                dt((2024, 4, 15), (23, 0, 0)),
                Some(600), // 4/15 23:00 → 4/16 09:00
            ),
            ev("201", dt((2024, 4, 16), (9, 0, 0)), Some(60)),
        ];
        let segs = build_segment_inputs(
            &events,
            dt((2024, 4, 15), (22, 0, 0)),
            dt((2024, 4, 16), (10, 0, 0)),
        );
        // 22:00〜23:00 と 9:00〜10:00 (actual_end は events 最終終了時刻 4/16 10:00) に分割
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].start, dt((2024, 4, 15), (22, 0, 0)));
        assert_eq!(segs[0].end, dt((2024, 4, 15), (23, 0, 0)));
        assert_eq!(segs[1].start, dt((2024, 4, 16), (9, 0, 0)));
    }
}

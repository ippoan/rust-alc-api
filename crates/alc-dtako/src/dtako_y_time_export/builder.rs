//! 1 ドライバー分の segment 列 (start, end, rest_minutes) を Y時間 シート行に
//! bucketing する pure logic。DB / R2 アクセスはここに含まない。

use chrono::{NaiveDate, NaiveDateTime, Timelike};
use std::collections::HashSet;

use super::models::YTimeRow;

#[derive(Debug, Clone, PartialEq)]
pub struct SegmentInput {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub rest_minutes: i32,
    pub note: Option<String>,
}

/// `segments` を bucketing して `YTimeRow` の列を返す。
///
/// 戻り値: `(rows, warnings)`
///
/// ルール:
/// - 同日 (start.date() == end.date()) → bucket = start.date()、F 空
/// - 翌日跨ぎ + その「終業日」に他 segment が始業している → bucket = end.date()、F=1
///   (1 暦日 2 始業ケース、終業日側に集約)
/// - 翌日跨ぎ + 終業日に他 segment なし → bucket = start.date()、F 空、H 列 24h+ 表記
/// - 期間外 (`bucket_date < from || > to`) は drop
/// - 同 bucket_date が複数: 最初の row のみ採用、警告 1 件
pub fn build_y_time_rows(
    mut segments: Vec<SegmentInput>,
    from: NaiveDate,
    to: NaiveDate,
) -> (Vec<YTimeRow>, Vec<String>) {
    segments.sort_by_key(|s| s.start);

    // 事前計算: 全 segment の start.date() の集合
    let start_dates: HashSet<NaiveDate> = segments.iter().map(|s| s.start.date()).collect();

    let mut rows: Vec<YTimeRow> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut seen_buckets: HashSet<NaiveDate> = HashSet::new();

    for seg in segments {
        let (bucket, previous_day_start, end_min_from_bucket) =
            if seg.start.date() == seg.end.date() {
                let end_min = minutes_of_day(seg.end);
                (seg.start.date(), false, end_min)
            } else if start_dates.contains(&seg.end.date()) {
                // 1 暦日 2 始業ケース: 終業日側に集約 (F=1)
                let end_min = minutes_of_day(seg.end);
                (seg.end.date(), true, end_min)
            } else {
                // 翌日跨ぎだが同日に他 segment なし → 24h+ 表記
                let bucket = seg.start.date();
                let bucket_midnight = bucket.and_hms_opt(0, 0, 0).expect("valid midnight");
                let end_min = (seg.end - bucket_midnight).num_minutes() as i32;
                (bucket, false, end_min)
            };

        // 期間外フィルタ
        if bucket < from || bucket > to {
            continue;
        }

        // 同 bucket_date が再出現 → drop + warning
        if !seen_buckets.insert(bucket) {
            warnings.push(format!(
                "{bucket}: 複数 segment 検出 (1 暦日 2 始業の後半は MVP では未対応、最初の row のみ採用)"
            ));
            continue;
        }

        rows.push(YTimeRow {
            date: bucket,
            previous_day_start,
            start_minutes_of_day: minutes_of_day(seg.start),
            end_minutes_from_bucket_date: end_min_from_bucket,
            rest_minutes: seg.rest_minutes,
            note: seg.note,
        });
    }

    rows.sort_by_key(|r| r.date);
    (rows, warnings)
}

fn minutes_of_day(dt: NaiveDateTime) -> i32 {
    dt.time().hour() as i32 * 60 + dt.time().minute() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(date: (i32, u32, u32), time: (u32, u32)) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(date.0, date.1, date.2)
            .unwrap()
            .and_hms_opt(time.0, time.1, 0)
            .unwrap()
    }

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn seg(
        start: NaiveDateTime,
        end: NaiveDateTime,
        rest: i32,
        note: Option<&str>,
    ) -> SegmentInput {
        SegmentInput {
            start,
            end,
            rest_minutes: rest,
            note: note.map(String::from),
        }
    }

    #[test]
    fn single_same_day_segment() {
        let s = seg(
            dt((2024, 4, 15), (8, 30)),
            dt((2024, 4, 15), (17, 0)),
            60,
            None,
        );
        let (rows, warns) = build_y_time_rows(vec![s], d(2024, 4, 1), d(2024, 4, 30));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].date, d(2024, 4, 15));
        assert!(!rows[0].previous_day_start);
        assert_eq!(rows[0].start_minutes_of_day, 8 * 60 + 30);
        assert_eq!(rows[0].end_minutes_from_bucket_date, 17 * 60);
        assert_eq!(rows[0].rest_minutes, 60);
        assert!(warns.is_empty());
    }

    #[test]
    fn cross_midnight_no_followup_uses_24h_plus() {
        // 4/15 22:30 → 4/16 06:00、4/16 に他 segment なし
        let s = seg(
            dt((2024, 4, 15), (22, 30)),
            dt((2024, 4, 16), (6, 0)),
            30,
            None,
        );
        let (rows, warns) = build_y_time_rows(vec![s], d(2024, 4, 1), d(2024, 4, 30));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].date, d(2024, 4, 15));
        assert!(!rows[0].previous_day_start);
        assert_eq!(rows[0].start_minutes_of_day, 22 * 60 + 30);
        // 30:00 == 30 * 60 == 1800
        assert_eq!(rows[0].end_minutes_from_bucket_date, 30 * 60);
        assert!(warns.is_empty());
    }

    #[test]
    fn cross_midnight_with_followup_uses_previous_day_flag() {
        // 4/15 22:30 → 4/16 09:30、続いて 4/16 17:00 → 4/16 22:00
        let s1 = seg(
            dt((2024, 4, 15), (22, 30)),
            dt((2024, 4, 16), (9, 30)),
            30,
            None,
        );
        let s2 = seg(
            dt((2024, 4, 16), (17, 0)),
            dt((2024, 4, 16), (22, 0)),
            0,
            None,
        );
        let (rows, warns) = build_y_time_rows(vec![s1, s2], d(2024, 4, 1), d(2024, 4, 30));
        // s1 は 4/16 row に F=1 で集約。s2 は同 4/16 bucket と衝突して warning + drop。
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].date, d(2024, 4, 16));
        assert!(rows[0].previous_day_start);
        assert_eq!(rows[0].start_minutes_of_day, 22 * 60 + 30);
        assert_eq!(rows[0].end_minutes_from_bucket_date, 9 * 60 + 30);
        assert_eq!(warns.len(), 1);
        assert!(warns[0].contains("2024-04-16"));
    }

    #[test]
    fn out_of_range_filtered() {
        // bucket=4/15 だが期間 4/16〜4/30 → drop
        let s = seg(
            dt((2024, 4, 15), (8, 0)),
            dt((2024, 4, 15), (17, 0)),
            0,
            None,
        );
        let (rows, _) = build_y_time_rows(vec![s], d(2024, 4, 16), d(2024, 4, 30));
        assert!(rows.is_empty());
    }

    #[test]
    fn multiple_distinct_days_sorted_ascending() {
        let segments = vec![
            seg(
                dt((2024, 4, 18), (8, 0)),
                dt((2024, 4, 18), (17, 0)),
                0,
                None,
            ),
            seg(
                dt((2024, 4, 15), (8, 0)),
                dt((2024, 4, 15), (17, 0)),
                0,
                None,
            ),
            seg(
                dt((2024, 4, 22), (8, 0)),
                dt((2024, 4, 22), (17, 0)),
                0,
                None,
            ),
        ];
        let (rows, warns) = build_y_time_rows(segments, d(2024, 4, 1), d(2024, 4, 30));
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].date, d(2024, 4, 15));
        assert_eq!(rows[1].date, d(2024, 4, 18));
        assert_eq!(rows[2].date, d(2024, 4, 22));
        assert!(warns.is_empty());
    }

    #[test]
    fn null_rest_handled_as_zero() {
        let s = seg(
            dt((2024, 4, 15), (8, 0)),
            dt((2024, 4, 15), (17, 0)),
            0,
            None,
        );
        let (rows, _) = build_y_time_rows(vec![s], d(2024, 4, 1), d(2024, 4, 30));
        assert_eq!(rows[0].rest_minutes, 0);
    }

    #[test]
    fn note_passes_through() {
        let s = seg(
            dt((2024, 4, 15), (8, 0)),
            dt((2024, 4, 15), (17, 0)),
            0,
            Some("テスト備考"),
        );
        let (rows, _) = build_y_time_rows(vec![s], d(2024, 4, 1), d(2024, 4, 30));
        assert_eq!(rows[0].note.as_deref(), Some("テスト備考"));
    }

    #[test]
    fn empty_input_yields_empty_output() {
        let (rows, warns) = build_y_time_rows(vec![], d(2024, 4, 1), d(2024, 4, 30));
        assert!(rows.is_empty());
        assert!(warns.is_empty());
    }
}

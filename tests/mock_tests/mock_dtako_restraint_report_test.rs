use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use chrono::{Datelike, NaiveDate, NaiveTime, TimeZone, Utc};
use serde_json::Value;

use rust_alc_api::db::repository::dtako_restraint_report::{
    DailyWorkHoursRow, DtakoRestraintReportRepository, OpTimesRow, SegmentRow,
};

// ============================================================
// Helper: spawn server with a given mock
// ============================================================

async fn spawn_with_mock(mock: Arc<dyn DtakoRestraintReportRepository>) -> (String, String, Uuid) {
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.dtako_restraint_report = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    (base_url, jwt, tenant_id)
}

fn auth(jwt: &str) -> String {
    format!("Bearer {jwt}")
}

// ============================================================
// GET /api/restraint-report — success (empty data, driver_name default)
// ============================================================

#[tokio::test]
async fn test_get_restraint_report_success_empty_data() {
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let driver_id = Uuid::new_v4();
    let res = client
        .get(format!(
            "{base_url}/api/restraint-report?driver_id={driver_id}&year=2026&month=3"
        ))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    assert_eq!(body["driver_name"], "");
    assert_eq!(body["year"], 2026);
    assert_eq!(body["month"], 3);
    assert!(body["days"].is_array());
    assert!(body["weekly_subtotals"].is_array());
    assert!(body["monthly_total"].is_object());
}

// ============================================================
// GET /api/restraint-report — success with driver name
// ============================================================

#[tokio::test]
async fn test_get_restraint_report_success_with_driver_name() {
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());
    mock.return_driver_name.store(true, Ordering::SeqCst);
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let driver_id = Uuid::new_v4();
    let res = client
        .get(format!(
            "{base_url}/api/restraint-report?driver_id={driver_id}&year=2026&month=6"
        ))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    assert_eq!(body["driver_name"], "テスト太郎");
    assert_eq!(body["year"], 2026);
    assert_eq!(body["month"], 6);
}

// ============================================================
// GET /api/restraint-report — missing query params → 400
// ============================================================

#[tokio::test]
async fn test_get_restraint_report_missing_params() {
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    // Missing all params
    let res = client
        .get(format!("{base_url}/api/restraint-report"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);

    // Missing year and month
    let driver_id = Uuid::new_v4();
    let res = client
        .get(format!(
            "{base_url}/api/restraint-report?driver_id={driver_id}"
        ))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);

    // Invalid driver_id (not a UUID)
    let res = client
        .get(format!(
            "{base_url}/api/restraint-report?driver_id=not-a-uuid&year=2026&month=3"
        ))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

// ============================================================
// GET /api/restraint-report — DB error → 500
// ============================================================

#[tokio::test]
async fn test_get_restraint_report_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let driver_id = Uuid::new_v4();
    let res = client
        .get(format!(
            "{base_url}/api/restraint-report?driver_id={driver_id}&year=2026&month=3"
        ))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// GET /api/restraint-report — unauthorized (no JWT) → 401
// ============================================================

#[tokio::test]
async fn test_get_restraint_report_unauthorized() {
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());
    let (base_url, _, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let driver_id = Uuid::new_v4();
    let res = client
        .get(format!(
            "{base_url}/api/restraint-report?driver_id={driver_id}&year=2026&month=3"
        ))
        .send()
        .await
        .unwrap();
    // require_tenant middleware: no JWT and no X-Tenant-ID → 401
    assert_eq!(res.status(), 401);
}

// ============================================================
// POST /api/restraint-report/compare-csv — no file → 400
// ============================================================

#[tokio::test]
async fn test_compare_csv_no_file() {
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    // Send empty multipart form
    let form = reqwest::multipart::Form::new();
    let res = client
        .post(format!("{base_url}/api/restraint-report/compare-csv"))
        .header("Authorization", auth(&jwt))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

// ============================================================
// POST /api/restraint-report/compare-csv — success (minimal CSV)
// ============================================================

#[tokio::test]
async fn test_compare_csv_success() {
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    // Minimal valid CSV matching parse_restraint_csv format
    let csv_content = "\
氏名,テスト太郎,,001
日付,休日,始業,終業,運転,重複運転,荷役,重複荷役,休憩,重複休憩,小計,重複小計,拘束合計,拘束累計,休息,実労,残業,深夜,残深夜,備考
2月1日,,8:00,17:00,1:00,,1:00,,0:30,,2:30,,2:30,2:30,,,,,,
合計,,,1:00,,1:00,,0:30,,,,2:30,,,,,,,,
";

    let part = reqwest::multipart::Part::bytes(csv_content.as_bytes().to_vec())
        .file_name("test.csv")
        .mime_str("text/csv")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/restraint-report/compare-csv"))
        .header("Authorization", auth(&jwt))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Vec<Value> = res.json().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["driver_name"], "テスト太郎");
    assert_eq!(body[0]["driver_cd"], "001");
    // No DB match → driver_id is null, system is null
    assert!(body[0]["driver_id"].is_null());
    assert!(body[0]["system"].is_null());
}

// ============================================================
// POST /api/restraint-report/compare-csv — DB error on list_drivers_with_cd → 500
// ============================================================

#[tokio::test]
async fn test_compare_csv_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());
    // fail_next will trigger on list_drivers_with_cd (the first DB call after CSV parse)
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let csv_content = "\
氏名,テスト太郎,,001
日付,休日,始業,終業,運転,重複運転,荷役,重複荷役,休憩,重複休憩,小計,重複小計,拘束合計,拘束累計,休息,実労,残業,深夜,残深夜,備考
2月1日,,8:00,17:00,1:00,,1:00,,0:30,,2:30,,2:30,2:30,,,,,,
合計,,,1:00,,1:00,,0:30,,,,2:30,,,,,,,,
";

    let part = reqwest::multipart::Part::bytes(csv_content.as_bytes().to_vec())
        .file_name("test.csv")
        .mime_str("text/csv")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/restraint-report/compare-csv"))
        .header("Authorization", auth(&jwt))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// POST /api/restraint-report/compare-csv — unauthorized → 401
// ============================================================

#[tokio::test]
async fn test_compare_csv_unauthorized() {
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());
    let (base_url, _, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let csv_content = "氏名,テスト,,001\n";
    let part = reqwest::multipart::Part::bytes(csv_content.as_bytes().to_vec())
        .file_name("test.csv")
        .mime_str("text/csv")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/restraint-report/compare-csv"))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

// ============================================================
// POST /api/restraint-report/compare-csv — with matching driver in DB
// ============================================================

#[tokio::test]
async fn test_compare_csv_with_matching_driver() {
    let driver_id = Uuid::new_v4();
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());
    // Set up list_drivers_with_cd to return a matching driver
    *mock.drivers_with_cd.lock().unwrap() =
        vec![(driver_id, Some("001".to_string()), "テスト太郎".to_string())];
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let csv_content = "\
氏名,テスト太郎,,001
日付,休日,始業,終業,運転,重複運転,荷役,重複荷役,休憩,重複休憩,小計,重複小計,拘束合計,拘束累計,休息,実労,残業,深夜,残深夜,備考
2月1日,,8:00,17:00,1:00,,1:00,,0:30,,2:30,,2:30,2:30,,,,,,
合計,,,1:00,,1:00,,0:30,,,,2:30,,,,,,,,
";

    let part = reqwest::multipart::Part::bytes(csv_content.as_bytes().to_vec())
        .file_name("test.csv")
        .mime_str("text/csv")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/restraint-report/compare-csv"))
        .header("Authorization", auth(&jwt))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Vec<Value> = res.json().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["driver_cd"], "001");
    // Driver matched → driver_id is set, system data is present
    assert_eq!(body[0]["driver_id"], driver_id.to_string());
    assert!(body[0]["system"].is_object());
}

// ============================================================
// Helper: build a mock with segment/dwh/op_times data for a given month
// ============================================================

fn make_segment(date: NaiveDate, unko_no: &str, work: i32, drive: i32, cargo: i32) -> SegmentRow {
    let start = Utc
        .with_ymd_and_hms(date.year(), date.month(), date.day(), 8, 0, 0)
        .unwrap();
    let end = Utc
        .with_ymd_and_hms(date.year(), date.month(), date.day(), 17, 0, 0)
        .unwrap();
    SegmentRow {
        work_date: date,
        unko_no: unko_no.to_string(),
        start_at: start,
        end_at: end,
        work_minutes: work,
        drive_minutes: drive,
        cargo_minutes: cargo,
    }
}

fn make_dwh(date: NaiveDate, drive: i32, cargo: i32, total_work: i32) -> DailyWorkHoursRow {
    DailyWorkHoursRow {
        work_date: date,
        start_time: NaiveTime::from_hms_opt(8, 0, 0).unwrap(),
        total_work_minutes: total_work,
        total_rest_minutes: Some(30),
        late_night_minutes: 10,
        drive_minutes: drive,
        cargo_minutes: cargo,
        overlap_drive_minutes: 5,
        overlap_cargo_minutes: 3,
        overlap_break_minutes: 2,
        overlap_restraint_minutes: 10,
        ot_late_night_minutes: 5,
    }
}

fn make_op_times(date: NaiveDate) -> OpTimesRow {
    let dep = Utc
        .with_ymd_and_hms(date.year(), date.month(), date.day(), 8, 15, 0)
        .unwrap();
    let end = Utc
        .with_ymd_and_hms(date.year(), date.month(), date.day(), 17, 30, 0)
        .unwrap();
    OpTimesRow {
        operation_date: date,
        first_departure: dep,
        last_seg_end: end,
    }
}

// ============================================================
// GET /api/restraint-report — invalid year/month → 400
// ============================================================

#[tokio::test]
async fn test_get_restraint_report_invalid_month() {
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let driver_id = Uuid::new_v4();
    // month=13 → invalid
    let res = client
        .get(format!(
            "{base_url}/api/restraint-report?driver_id={driver_id}&year=2026&month=13"
        ))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    let body = res.text().await.unwrap();
    assert!(body.contains("invalid year/month"), "body: {body}");
}

// ============================================================
// GET /api/restraint-report — month=12 (December boundary)
// ============================================================

#[tokio::test]
async fn test_get_restraint_report_december() {
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let driver_id = Uuid::new_v4();
    let res = client
        .get(format!(
            "{base_url}/api/restraint-report?driver_id={driver_id}&year=2025&month=12"
        ))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    assert_eq!(body["year"], 2025);
    assert_eq!(body["month"], 12);
    // December should have 31 days
    assert_eq!(body["days"].as_array().unwrap().len(), 31);
}

// ============================================================
// GET /api/restraint-report — with segments + dwh + op_times data
// ============================================================

#[tokio::test]
async fn test_get_restraint_report_with_rich_data() {
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());
    mock.return_driver_name.store(true, Ordering::SeqCst);

    let d1 = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(); // Monday
    let d2 = NaiveDate::from_ymd_opt(2026, 1, 6).unwrap(); // Tuesday

    // Segments: 2 operations on day 1, 1 on day 2
    *mock.segments.lock().unwrap() = vec![
        make_segment(d1, "OP001", 480, 300, 120),
        make_segment(d1, "OP002", 120, 60, 30),
        make_segment(d2, "OP003", 540, 360, 100),
    ];

    // Daily work hours
    *mock.daily_work_hours.lock().unwrap() =
        vec![make_dwh(d1, 360, 150, 600), make_dwh(d2, 360, 100, 540)];

    // Operation times
    *mock.op_times.lock().unwrap() = vec![make_op_times(d1), make_op_times(d2)];

    // Previous day drive
    *mock.prev_day_drive.lock().unwrap() = Some(200);

    // Fiscal cumulative (January → fiscal year start is April of previous year)
    *mock.fiscal_cumulative.lock().unwrap() = 5000;

    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let driver_id = Uuid::new_v4();
    let res = client
        .get(format!(
            "{base_url}/api/restraint-report?driver_id={driver_id}&year=2026&month=1"
        ))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    assert_eq!(body["driver_name"], "テスト太郎");
    assert_eq!(body["year"], 2026);
    assert_eq!(body["month"], 1);
    assert_eq!(body["max_restraint_minutes"], 16500);

    // Check days array has 31 entries (January)
    let days = body["days"].as_array().unwrap();
    assert_eq!(days.len(), 31);

    // Day 5 (index 4) should have data
    let day5 = &days[4];
    assert!(!day5["is_holiday"].as_bool().unwrap());
    assert_eq!(day5["start_time"], "8:15");
    assert_eq!(day5["end_time"], "17:30");
    assert!(day5["drive_minutes"].as_i64().unwrap() > 0);
    assert!(day5["cargo_minutes"].as_i64().unwrap() > 0);
    assert!(day5["restraint_total_minutes"].as_i64().unwrap() > 0);
    assert!(day5["restraint_cumulative_minutes"].as_i64().unwrap() > 0);
    assert!(day5["overlap_drive_minutes"].as_i64().unwrap() > 0);
    assert!(day5["overlap_cargo_minutes"].as_i64().unwrap() > 0);
    assert!(day5["overlap_break_minutes"].as_i64().unwrap() > 0);
    assert!(day5["overlap_restraint_minutes"].as_i64().unwrap() > 0);
    assert!(day5["actual_work_minutes"].as_i64().unwrap() > 0);
    assert!(day5["restraint_main_minutes"].as_i64().unwrap() > 0);
    // drive_avg_before should be set (prev_day_drive=200)
    assert!(day5["drive_avg_before"].is_number());
    // drive_avg_after should be set (pass 2)
    assert!(day5["drive_avg_after"].is_number());

    // Day 6 (index 5) should also have data
    let day6 = &days[5];
    assert!(!day6["is_holiday"].as_bool().unwrap());
    assert!(day6["drive_minutes"].as_i64().unwrap() > 0);

    // Operations array on day 5 should have 2 entries (OP001, OP002)
    let ops = day5["operations"].as_array().unwrap();
    assert_eq!(ops.len(), 2);

    // Holiday days should have remarks = "休"
    let day1 = &days[0]; // Jan 1
    assert!(day1["is_holiday"].as_bool().unwrap());
    assert_eq!(day1["remarks"], "休");

    // Weekly subtotals should exist (we have work data)
    let ws = body["weekly_subtotals"].as_array().unwrap();
    assert!(!ws.is_empty());
    // At least one weekly subtotal has positive restraint
    assert!(ws
        .iter()
        .any(|w| w["restraint_minutes"].as_i64().unwrap() > 0));

    // Monthly total
    let mt = &body["monthly_total"];
    assert!(mt["drive_minutes"].as_i64().unwrap() > 0);
    assert!(mt["restraint_minutes"].as_i64().unwrap() > 0);
    assert_eq!(mt["fiscal_year_cumulative_minutes"], 5000);
    assert!(mt["fiscal_year_total_minutes"].as_i64().unwrap() > 5000);
}

// ============================================================
// GET /api/restraint-report — multiple dwh rows on same day
// ============================================================

#[tokio::test]
async fn test_get_restraint_report_multiple_dwh_same_day() {
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());

    let d1 = NaiveDate::from_ymd_opt(2026, 3, 2).unwrap();

    *mock.segments.lock().unwrap() = vec![make_segment(d1, "OP001", 480, 300, 120)];

    // Two DWH rows on the same day (representing multi-shift)
    let mut dwh1 = make_dwh(d1, 200, 80, 350);
    dwh1.start_time = NaiveTime::from_hms_opt(1, 17, 0).unwrap();
    let mut dwh2 = make_dwh(d1, 160, 70, 300);
    dwh2.start_time = NaiveTime::from_hms_opt(23, 17, 0).unwrap();
    *mock.daily_work_hours.lock().unwrap() = vec![dwh1, dwh2];

    *mock.op_times.lock().unwrap() = vec![make_op_times(d1)];

    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let driver_id = Uuid::new_v4();
    let res = client
        .get(format!(
            "{base_url}/api/restraint-report?driver_id={driver_id}&year=2026&month=3"
        ))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    let days = body["days"].as_array().unwrap();

    // Day 2 should generate 2 rows (multi-dwh), so total > 31
    // Count rows for March 2 (index 1 in original, but with 2 dwh rows it creates 2 entries)
    let march2_rows: Vec<&Value> = days
        .iter()
        .filter(|d| d["date"].as_str().unwrap() == "2026-03-02")
        .collect();
    assert_eq!(
        march2_rows.len(),
        2,
        "Should have 2 rows for same-day multi-DWH"
    );

    // First row should have operations, second should have empty operations
    assert!(!march2_rows[0]["operations"].as_array().unwrap().is_empty());
    assert!(march2_rows[1]["operations"].as_array().unwrap().is_empty());

    // First row should have start_time from dwh (1:17)
    assert_eq!(march2_rows[0]["start_time"], "1:17");
    // Second row start_time should be from dwh2 (23:17)
    assert_eq!(march2_rows[1]["start_time"], "23:17");
}

// ============================================================
// GET /api/restraint-report — segments without dwh (fallback to segment sums)
// ============================================================

#[tokio::test]
async fn test_get_restraint_report_segments_no_dwh() {
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());

    let d1 = NaiveDate::from_ymd_opt(2026, 2, 10).unwrap();

    // Segments present but no DWH rows
    *mock.segments.lock().unwrap() = vec![
        make_segment(d1, "OP001", 500, 300, 120),
        make_segment(d1, "OP002", 200, 100, 60),
    ];

    // No dwh, no op_times → fallback to segment start/end
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let driver_id = Uuid::new_v4();
    let res = client
        .get(format!(
            "{base_url}/api/restraint-report?driver_id={driver_id}&year=2026&month=2"
        ))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    let days = body["days"].as_array().unwrap();
    assert_eq!(days.len(), 28); // Feb 2026

    // Day 10 (index 9) should have data from segments
    let day10 = &days[9];
    assert!(!day10["is_holiday"].as_bool().unwrap());
    // drive = sum of segment drives = 300 + 100 = 400
    assert_eq!(day10["drive_minutes"], 400);
    // cargo = sum of segment cargos = 120 + 60 = 180
    assert_eq!(day10["cargo_minutes"], 180);
    // restraint = sum of segment work = 500 + 200 = 700
    assert_eq!(day10["restraint_main_minutes"], 700);
    // No overlap
    assert_eq!(day10["overlap_drive_minutes"], 0);
    // start_time/end_time from segment timestamps (fallback)
    assert!(day10["start_time"].is_string());
    assert!(day10["end_time"].is_string());
    // Operations should have 2 entries
    assert_eq!(day10["operations"].as_array().unwrap().len(), 2);
}

// ============================================================
// GET /api/restraint-report — fiscal year boundary (month=4, April)
// ============================================================

#[tokio::test]
async fn test_get_restraint_report_fiscal_year_april() {
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());
    // April: fiscal_year_start = same year April → prev_month_end = March 31
    // fiscal_year_start <= prev_month_end is false (April 1 > March 31 is false, equal)
    // Actually: fiscal_year_start = April 1, prev_month_end = March 31
    // April 1 <= March 31 is false → fiscal_cum = 0 (else branch)
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let driver_id = Uuid::new_v4();
    let res = client
        .get(format!(
            "{base_url}/api/restraint-report?driver_id={driver_id}&year=2026&month=4"
        ))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    // For April (first month of fiscal year), cumulative should be 0
    assert_eq!(body["monthly_total"]["fiscal_year_cumulative_minutes"], 0);
}

// ============================================================
// GET /api/restraint-report — with Sunday boundary for weekly subtotals
// ============================================================

#[tokio::test]
async fn test_get_restraint_report_weekly_sunday_boundary() {
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());

    // March 2026: March 1 is Sunday
    // We need data spanning across a Sunday boundary
    // Let's use January 2026: Jan 1 = Thursday, Jan 4 = Sunday
    let d2 = NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(); // Fri
    let d3 = NaiveDate::from_ymd_opt(2026, 1, 3).unwrap(); // Sat
    let d5 = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(); // Mon (after Sunday)
    let d6 = NaiveDate::from_ymd_opt(2026, 1, 6).unwrap(); // Tue

    *mock.segments.lock().unwrap() = vec![
        make_segment(d2, "OP001", 480, 300, 120),
        make_segment(d3, "OP002", 400, 250, 100),
        make_segment(d5, "OP003", 500, 320, 130),
        make_segment(d6, "OP004", 450, 280, 110),
    ];

    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let driver_id = Uuid::new_v4();
    let res = client
        .get(format!(
            "{base_url}/api/restraint-report?driver_id={driver_id}&year=2026&month=1"
        ))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    let ws = body["weekly_subtotals"].as_array().unwrap();
    // Should have at least 2 weekly subtotals (before and after Sunday boundary)
    assert!(
        ws.len() >= 2,
        "Expected at least 2 weekly subtotals, got {}",
        ws.len()
    );
    // Each subtotal should have positive restraint
    for w in ws {
        assert!(w["restraint_minutes"].as_i64().unwrap() > 0);
        assert!(w["drive_minutes"].as_i64().unwrap() > 0);
    }
}

// ============================================================
// GET /api/restraint-report — drive_avg_after calculation
// ============================================================

#[tokio::test]
async fn test_get_restraint_report_drive_avg_after() {
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());

    // Two consecutive working days
    let d1 = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
    let d2 = NaiveDate::from_ymd_opt(2026, 6, 2).unwrap();

    *mock.segments.lock().unwrap() = vec![
        make_segment(d1, "OP001", 600, 400, 100),
        make_segment(d2, "OP002", 480, 300, 80),
    ];

    *mock.daily_work_hours.lock().unwrap() =
        vec![make_dwh(d1, 400, 100, 600), make_dwh(d2, 300, 80, 480)];

    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let driver_id = Uuid::new_v4();
    let res = client
        .get(format!(
            "{base_url}/api/restraint-report?driver_id={driver_id}&year=2026&month=6"
        ))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    let days = body["days"].as_array().unwrap();

    // Day 1 (index 0): drive=400, next day drive=300 → after = (400+300)/2 = 350
    let day1 = &days[0];
    assert_eq!(day1["drive_avg_after"], 350);

    // Day 2 (index 1): drive=300, next day is holiday (drive=0) → after = (300+0)/2 = 150
    let day2 = &days[1];
    assert_eq!(day2["drive_avg_after"], 150);
}

// ============================================================
// GET /api/restraint-report — overtime and late_night calculations
// ============================================================

#[tokio::test]
async fn test_get_restraint_report_overtime_calculation() {
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());

    let d1 = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();

    *mock.segments.lock().unwrap() = vec![make_segment(d1, "OP001", 600, 400, 200)];

    // DWH with actual_work > 480 → overtime
    // drive=400, cargo=200, actual_work=600, overtime = (600-480)=120, ot_late_night=5 → overtime=115
    let mut dwh = make_dwh(d1, 400, 200, 600);
    dwh.ot_late_night_minutes = 20;
    dwh.late_night_minutes = 30;
    dwh.total_rest_minutes = Some(60);
    *mock.daily_work_hours.lock().unwrap() = vec![dwh];

    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let driver_id = Uuid::new_v4();
    let res = client
        .get(format!(
            "{base_url}/api/restraint-report?driver_id={driver_id}&year=2026&month=5"
        ))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    let days = body["days"].as_array().unwrap();
    let day1 = &days[0];

    // actual_work = drive + cargo = 400 + 200 = 600
    assert_eq!(day1["actual_work_minutes"], 600);
    // total_overtime = (600 - 480).max(0) = 120
    // overtime = (120 - 20).max(0) = 100
    assert_eq!(day1["overtime_minutes"], 100);
    assert_eq!(day1["late_night_minutes"], 30);
    assert_eq!(day1["overtime_late_night_minutes"], 20);
    // rest_period should be 60
    assert_eq!(day1["rest_period_minutes"], 60);
}

// ============================================================
// POST /api/restraint-report/compare-csv — with driver_cd filter
// ============================================================

#[tokio::test]
async fn test_compare_csv_with_driver_cd_filter() {
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    // CSV with driver_cd=001
    let csv_content = "\
氏名,テスト太郎,,001
日付,休日,始業,終業,運転,重複運転,荷役,重複荷役,休憩,重複休憩,小計,重複小計,拘束合計,拘束累計,休息,実労,残業,深夜,残深夜,備考
2月1日,,8:00,17:00,1:00,,1:00,,0:30,,2:30,,2:30,2:30,,,,,,
合計,,,1:00,,1:00,,0:30,,,,2:30,,,,,,,,
";

    let part = reqwest::multipart::Part::bytes(csv_content.as_bytes().to_vec())
        .file_name("test.csv")
        .mime_str("text/csv")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    // Filter for driver_cd=999 → no match → empty results
    let res = client
        .post(format!(
            "{base_url}/api/restraint-report/compare-csv?driver_cd=999"
        ))
        .header("Authorization", auth(&jwt))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Vec<Value> = res.json().await.unwrap();
    assert_eq!(
        body.len(),
        0,
        "driver_cd filter should exclude non-matching drivers"
    );

    // Now filter for driver_cd=001 → should match
    let part2 = reqwest::multipart::Part::bytes(csv_content.as_bytes().to_vec())
        .file_name("test.csv")
        .mime_str("text/csv")
        .unwrap();
    let form2 = reqwest::multipart::Form::new().part("file", part2);

    let res2 = client
        .post(format!(
            "{base_url}/api/restraint-report/compare-csv?driver_cd=001"
        ))
        .header("Authorization", auth(&jwt))
        .multipart(form2)
        .send()
        .await
        .unwrap();
    assert_eq!(res2.status(), 200);

    let body2: Vec<Value> = res2.json().await.unwrap();
    assert_eq!(body2.len(), 1);
    assert_eq!(body2[0]["driver_cd"], "001");
}

// ============================================================
// POST /api/restraint-report/compare-csv — multipart with no file field → empty bytes → 400
// ============================================================

#[tokio::test]
async fn test_compare_csv_empty_file_field() {
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    // Send a file field with empty content
    let part = reqwest::multipart::Part::bytes(Vec::new())
        .file_name("empty.csv")
        .mime_str("text/csv")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/restraint-report/compare-csv"))
        .header("Authorization", auth(&jwt))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

// ============================================================
// POST /api/restraint-report/compare-csv — with matching driver + system data + diffs
// ============================================================

#[tokio::test]
async fn test_compare_csv_with_system_data_and_diffs() {
    let driver_id = Uuid::new_v4();
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());
    mock.return_driver_name.store(true, Ordering::SeqCst);

    // Set up matching driver
    *mock.drivers_with_cd.lock().unwrap() =
        vec![(driver_id, Some("001".to_string()), "テスト太郎".to_string())];

    // Add segment data so system report has non-zero values
    let d1 = NaiveDate::from_ymd_opt(2026, 2, 1).unwrap();
    *mock.segments.lock().unwrap() = vec![make_segment(d1, "OP001", 480, 300, 120)];
    *mock.daily_work_hours.lock().unwrap() = vec![make_dwh(d1, 300, 120, 480)];
    *mock.op_times.lock().unwrap() = vec![make_op_times(d1)];

    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    // CSV that intentionally differs from system data to generate diffs
    let csv_content = "\
氏名,テスト太郎,,001
日付,休日,始業,終業,運転,重複運転,荷役,重複荷役,休憩,重複休憩,小計,重複小計,拘束合計,拘束累計,休息,実労,残業,深夜,残深夜,備考
2月1日,,8:00,17:00,9:99,,1:00,,0:30,,2:30,,2:30,2:30,,,,,,
合計,,,9:99,,1:00,,0:30,,,,2:30,,,,,,,,
";

    let part = reqwest::multipart::Part::bytes(csv_content.as_bytes().to_vec())
        .file_name("test.csv")
        .mime_str("text/csv")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/restraint-report/compare-csv"))
        .header("Authorization", auth(&jwt))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Vec<Value> = res.json().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["driver_id"], driver_id.to_string());
    assert!(body[0]["system"].is_object());
    // System data should have days
    let sys = &body[0]["system"];
    assert!(sys["days"].is_array());
    let sys_days = sys["days"].as_array().unwrap();
    assert!(!sys_days.is_empty());
    // System total fields should be present
    assert!(sys["total_drive"].is_string());
    assert!(sys["total_restraint"].is_string());

    // Diffs should exist (CSV drive "9:99" vs system drive)
    let diffs = body[0]["diffs"].as_array().unwrap();
    assert!(
        !diffs.is_empty(),
        "Should have diffs between CSV and system data"
    );
    // known_bug_diffs and unknown_diffs fields should be present
    assert!(body[0]["known_bug_diffs"].is_number());
    assert!(body[0]["unknown_diffs"].is_number());
}

// ============================================================
// POST /api/restraint-report/compare-csv — matching driver but build_report fails
// ============================================================

#[tokio::test]
async fn test_compare_csv_matching_driver_report_error() {
    let driver_id = Uuid::new_v4();
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());

    // Set up matching driver but DON'T set fail_next (fail_next would fail list_drivers_with_cd)
    // Instead, the mock returns empty data, so the report succeeds but has no segments
    *mock.drivers_with_cd.lock().unwrap() =
        vec![(driver_id, Some("001".to_string()), "テスト太郎".to_string())];

    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let csv_content = "\
氏名,テスト太郎,,001
日付,休日,始業,終業,運転,重複運転,荷役,重複荷役,休憩,重複休憩,小計,重複小計,拘束合計,拘束累計,休息,実労,残業,深夜,残深夜,備考
2月1日,,8:00,17:00,1:00,,1:00,,0:30,,2:30,,2:30,2:30,,,,,,
合計,,,1:00,,1:00,,0:30,,,,2:30,,,,,,,,
";

    let part = reqwest::multipart::Part::bytes(csv_content.as_bytes().to_vec())
        .file_name("test.csv")
        .mime_str("text/csv")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/restraint-report/compare-csv"))
        .header("Authorization", auth(&jwt))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Vec<Value> = res.json().await.unwrap();
    assert_eq!(body.len(), 1);
    // System data should be present (empty report, not error)
    assert!(body[0]["system"].is_object());
}

// ============================================================
// GET /api/restraint-report — month before April (fiscal year previous year)
// ============================================================

#[tokio::test]
async fn test_get_restraint_report_january_fiscal_year() {
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());
    // January → fiscal_year_start = previous year April
    *mock.fiscal_cumulative.lock().unwrap() = 12000;

    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let driver_id = Uuid::new_v4();
    let res = client
        .get(format!(
            "{base_url}/api/restraint-report?driver_id={driver_id}&year=2026&month=1"
        ))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    assert_eq!(
        body["monthly_total"]["fiscal_year_cumulative_minutes"],
        12000
    );
}

// ============================================================
// POST /api/restraint-report/compare-csv — multiple drivers in CSV
// ============================================================

#[tokio::test]
async fn test_compare_csv_multiple_drivers() {
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    // Two drivers in one CSV
    let csv_content = "\
氏名,ドライバーA,,001
日付,休日,始業,終業,運転,重複運転,荷役,重複荷役,休憩,重複休憩,小計,重複小計,拘束合計,拘束累計,休息,実労,残業,深夜,残深夜,備考
2月1日,,8:00,17:00,1:00,,1:00,,0:30,,2:30,,2:30,2:30,,,,,,
合計,,,1:00,,1:00,,0:30,,,,2:30,,,,,,,,
氏名,ドライバーB,,002
日付,休日,始業,終業,運転,重複運転,荷役,重複荷役,休憩,重複休憩,小計,重複小計,拘束合計,拘束累計,休息,実労,残業,深夜,残深夜,備考
2月1日,,9:00,18:00,2:00,,0:30,,0:15,,2:45,,2:45,2:45,,,,,,
合計,,,2:00,,0:30,,0:15,,,,2:45,,,,,,,,
";

    let part = reqwest::multipart::Part::bytes(csv_content.as_bytes().to_vec())
        .file_name("test.csv")
        .mime_str("text/csv")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/restraint-report/compare-csv"))
        .header("Authorization", auth(&jwt))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Vec<Value> = res.json().await.unwrap();
    assert_eq!(body.len(), 2);
    assert_eq!(body[0]["driver_cd"], "001");
    assert_eq!(body[1]["driver_cd"], "002");
}

// ============================================================
// GET /api/restraint-report — rest_period_minutes = 0 should be None
// ============================================================

#[tokio::test]
async fn test_get_restraint_report_rest_period_zero_filtered() {
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());

    let d1 = NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();

    *mock.segments.lock().unwrap() = vec![make_segment(d1, "OP001", 480, 300, 120)];

    let mut dwh = make_dwh(d1, 300, 120, 480);
    dwh.total_rest_minutes = Some(0); // 0 should be filtered to None
    *mock.daily_work_hours.lock().unwrap() = vec![dwh];

    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let driver_id = Uuid::new_v4();
    let res = client
        .get(format!(
            "{base_url}/api/restraint-report?driver_id={driver_id}&year=2026&month=7"
        ))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    let days = body["days"].as_array().unwrap();
    let day1 = &days[0];
    // rest_period_minutes = Some(0) filtered by .filter(|&v| v > 0) → None → null
    assert!(day1["rest_period_minutes"].is_null());
}

// ============================================================
// GET /api/restraint-report — no prev_day_drive → drive_avg = day_drive
// ============================================================

#[tokio::test]
async fn test_get_restraint_report_no_prev_day_drive() {
    let mock = Arc::new(crate::mock_helpers::MockDtakoRestraintReportRepository::default());

    let d1 = NaiveDate::from_ymd_opt(2026, 8, 1).unwrap();

    *mock.segments.lock().unwrap() = vec![make_segment(d1, "OP001", 480, 300, 120)];
    *mock.daily_work_hours.lock().unwrap() = vec![make_dwh(d1, 300, 120, 480)];
    // prev_day_drive = None (default)

    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let driver_id = Uuid::new_v4();
    let res = client
        .get(format!(
            "{base_url}/api/restraint-report?driver_id={driver_id}&year=2026&month=8"
        ))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    let days = body["days"].as_array().unwrap();
    let day1 = &days[0];
    // No prev_day_drive → drive_avg_before = None
    assert!(day1["drive_avg_before"].is_null());
    // drive_average = day_drive as f64 = 300.0
    assert_eq!(day1["drive_average_minutes"].as_f64().unwrap(), 300.0);
}

#[macro_use]
mod common;
mod mock_helpers;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde_json::Value;
use uuid::Uuid;

use rust_alc_api::db::repository::dtako_restraint_report::DtakoRestraintReportRepository;

// ============================================================
// Helper: spawn server with a given mock
// ============================================================

async fn spawn_with_mock(mock: Arc<dyn DtakoRestraintReportRepository>) -> (String, String, Uuid) {
    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");

    let mut state = mock_helpers::app_state::setup_mock_app_state().await;
    state.dtako_restraint_report = mock;
    let base_url = common::spawn_test_server(state).await;

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
    let mock = Arc::new(mock_helpers::MockDtakoRestraintReportRepository::default());
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
    let mock = Arc::new(mock_helpers::MockDtakoRestraintReportRepository::default());
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
    let mock = Arc::new(mock_helpers::MockDtakoRestraintReportRepository::default());
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
    let mock = Arc::new(mock_helpers::MockDtakoRestraintReportRepository::default());
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
    let mock = Arc::new(mock_helpers::MockDtakoRestraintReportRepository::default());
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
    let mock = Arc::new(mock_helpers::MockDtakoRestraintReportRepository::default());
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
    let mock = Arc::new(mock_helpers::MockDtakoRestraintReportRepository::default());
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
    let mock = Arc::new(mock_helpers::MockDtakoRestraintReportRepository::default());
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
    let mock = Arc::new(mock_helpers::MockDtakoRestraintReportRepository::default());
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
    let mock = Arc::new(mock_helpers::MockDtakoRestraintReportRepository::default());
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

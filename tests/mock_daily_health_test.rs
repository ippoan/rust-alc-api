mod common;
mod mock_helpers;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use mock_helpers::app_state::setup_mock_app_state;
use mock_helpers::MockDailyHealthRepository;

// ---------------------------------------------------------------------------
// GET /api/tenko/daily-health-status — success (empty employees, default date)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_daily_health_status_success_empty() {
    let state = setup_mock_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "MockDHEmpty").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/daily-health-status"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();

    // Mock returns empty vec, so all summary counts should be 0
    assert!(body["employees"].is_array());
    assert_eq!(body["employees"].as_array().unwrap().len(), 0);
    assert_eq!(body["summary"]["total_employees"], 0);
    assert_eq!(body["summary"]["checked_count"], 0);
    assert_eq!(body["summary"]["unchecked_count"], 0);
    assert_eq!(body["summary"]["pass_count"], 0);
    assert_eq!(body["summary"]["fail_count"], 0);

    // date field should be present (JST today)
    assert!(body["date"].is_string());
}

// ---------------------------------------------------------------------------
// GET /api/tenko/daily-health-status?date=2026-03-15 — success with explicit date
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_daily_health_status_with_date_param() {
    let state = setup_mock_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "MockDHDate").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!(
            "{base_url}/api/tenko/daily-health-status?date=2026-03-15"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["date"], "2026-03-15");
    assert!(body["employees"].is_array());
}

// ---------------------------------------------------------------------------
// GET /api/tenko/daily-health-status — no auth → 401
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_daily_health_status_no_auth() {
    let state = setup_mock_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/daily-health-status"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

// ---------------------------------------------------------------------------
// GET /api/tenko/daily-health-status — X-Tenant-ID header (kiosk mode) → 200
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_daily_health_status_tenant_header() {
    let state = setup_mock_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "MockDHTenant").await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/daily-health-status"))
        .header("X-Tenant-ID", tenant_id.to_string())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["employees"].is_array());
    assert!(body["summary"].is_object());
}

// ---------------------------------------------------------------------------
// GET /api/tenko/daily-health-status — DB error (fail_next) → 500
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_daily_health_status_db_error() {
    let mut state = setup_mock_app_state().await;

    // Replace daily_health with a mock that will fail on next call
    let mock = Arc::new(MockDailyHealthRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    state.daily_health = mock;

    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "MockDHErr").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/daily-health-status"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// ---------------------------------------------------------------------------
// GET /api/tenko/daily-health-status — with safety_judgment pass/fail records
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_daily_health_status_with_safety_judgment() {
    use rust_alc_api::routes::daily_health::DailyHealthRow;
    use serde_json::json;
    use uuid::Uuid;

    let mut state = setup_mock_app_state().await;

    // Build mock data with safety_judgment pass, fail, and a checked record without judgment
    let rows = vec![
        DailyHealthRow {
            employee_id: Uuid::new_v4(),
            employee_name: "Pass Employee".to_string(),
            employee_code: Some("E001".to_string()),
            session_id: Some(Uuid::new_v4()),
            tenko_type: Some("pre_operation".to_string()),
            completed_at: Some(chrono::Utc::now()),
            temperature: Some(36.5),
            systolic: Some(120),
            diastolic: Some(80),
            pulse: Some(72),
            medical_measured_at: Some(chrono::Utc::now()),
            medical_manual_input: Some(false),
            alcohol_result: Some("negative".to_string()),
            alcohol_value: Some(0.0),
            self_declaration: None,
            safety_judgment: Some(json!({"status": "pass"})),
            has_baseline: Some(true),
            baseline_systolic: Some(120),
            baseline_diastolic: Some(80),
            baseline_temperature: Some(36.5),
            systolic_tolerance: Some(20),
            diastolic_tolerance: Some(15),
            temperature_tolerance: Some(0.5),
        },
        DailyHealthRow {
            employee_id: Uuid::new_v4(),
            employee_name: "Fail Employee".to_string(),
            employee_code: Some("E002".to_string()),
            session_id: Some(Uuid::new_v4()),
            tenko_type: Some("pre_operation".to_string()),
            completed_at: Some(chrono::Utc::now()),
            temperature: Some(38.0),
            systolic: Some(160),
            diastolic: Some(100),
            pulse: Some(95),
            medical_measured_at: Some(chrono::Utc::now()),
            medical_manual_input: Some(false),
            alcohol_result: Some("negative".to_string()),
            alcohol_value: Some(0.0),
            self_declaration: None,
            safety_judgment: Some(json!({"status": "fail"})),
            has_baseline: Some(true),
            baseline_systolic: Some(120),
            baseline_diastolic: Some(80),
            baseline_temperature: Some(36.5),
            systolic_tolerance: Some(20),
            diastolic_tolerance: Some(15),
            temperature_tolerance: Some(0.5),
        },
        DailyHealthRow {
            employee_id: Uuid::new_v4(),
            employee_name: "Unchecked Employee".to_string(),
            employee_code: Some("E003".to_string()),
            session_id: None,
            tenko_type: None,
            completed_at: None,
            temperature: None,
            systolic: None,
            diastolic: None,
            pulse: None,
            medical_measured_at: None,
            medical_manual_input: None,
            alcohol_result: None,
            alcohol_value: None,
            self_declaration: None,
            safety_judgment: None,
            has_baseline: None,
            baseline_systolic: None,
            baseline_diastolic: None,
            baseline_temperature: None,
            systolic_tolerance: None,
            diastolic_tolerance: None,
            temperature_tolerance: None,
        },
    ];

    let mock = Arc::new(MockDailyHealthRepository {
        fail_next: std::sync::atomic::AtomicBool::new(false),
        data: std::sync::Mutex::new(rows),
    });
    state.daily_health = mock;

    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "MockDHJudge").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/daily-health-status"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();

    assert_eq!(body["summary"]["total_employees"], 3);
    assert_eq!(body["summary"]["checked_count"], 2);
    assert_eq!(body["summary"]["unchecked_count"], 1);
    assert_eq!(body["summary"]["pass_count"], 1);
    assert_eq!(body["summary"]["fail_count"], 1);
    assert_eq!(body["employees"].as_array().unwrap().len(), 3);
}

// ---------------------------------------------------------------------------
// GET /api/tenko/daily-health-status?date=invalid — bad date format → 400
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_daily_health_status_invalid_date() {
    let state = setup_mock_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "MockDHBadDate").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!(
            "{base_url}/api/tenko/daily-health-status?date=not-a-date"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    // Invalid query param deserialization should return 400
    assert_eq!(res.status(), 400);
}

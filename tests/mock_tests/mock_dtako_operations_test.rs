use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use chrono::{NaiveDate, Utc};

use crate::mock_helpers::app_state::setup_mock_app_state;
use crate::mock_helpers::MockDtakoOperationsRepository;
use rust_alc_api::db::models::DtakoOperation;

fn make_operation(unko_no: &str) -> DtakoOperation {
    DtakoOperation {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        unko_no: unko_no.to_string(),
        crew_role: 1,
        reading_date: NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(),
        operation_date: Some(NaiveDate::from_ymd_opt(2026, 3, 1).unwrap()),
        office_id: None,
        vehicle_id: None,
        driver_id: None,
        departure_at: None,
        return_at: None,
        garage_out_at: None,
        garage_in_at: None,
        meter_start: None,
        meter_end: None,
        total_distance: Some(100.0),
        drive_time_general: Some(120),
        drive_time_highway: Some(30),
        drive_time_bypass: Some(10),
        safety_score: Some(85.0),
        economy_score: Some(90.0),
        total_score: Some(87.5),
        raw_data: serde_json::json!({}),
        r2_key_prefix: None,
        uploaded_at: Utc::now(),
        has_kudgivt: true,
    }
}

// ---------------------------------------------------------------------------
// GET /api/operations — success (empty list)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_operations_success() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/operations"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["operations"].as_array().unwrap().len(), 0);
    assert_eq!(body["total"], 0);
    assert_eq!(body["page"], 1);
    assert_eq!(body["per_page"], 50);
}

// ---------------------------------------------------------------------------
// GET /api/operations — no auth → 401
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_operations_no_auth() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/operations"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

// ---------------------------------------------------------------------------
// GET /api/operations — X-Tenant-ID header (kiosk mode) → 200
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_operations_tenant_header() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = Uuid::new_v4();
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/operations"))
        .header("X-Tenant-ID", tenant_id.to_string())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
}

// ---------------------------------------------------------------------------
// GET /api/operations — DB error → 500
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_operations_db_error() {
    let mut state = setup_mock_app_state();
    let mock = Arc::new(MockDtakoOperationsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    state.dtako_operations = mock;

    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/operations"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// ---------------------------------------------------------------------------
// GET /api/operations/calendar — success (empty)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_calendar_dates_success_empty() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!(
            "{base_url}/api/operations/calendar?year=2026&month=3"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["year"], 2026);
    assert_eq!(body["month"], 3);
    assert_eq!(body["dates"].as_array().unwrap().len(), 0);
}

// ---------------------------------------------------------------------------
// GET /api/operations/calendar — success with data
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_calendar_dates_success_with_data() {
    let mut state = setup_mock_app_state();
    let mock = Arc::new(MockDtakoOperationsRepository::default());
    *mock.calendar_dates_result.lock().unwrap() = vec![
        (NaiveDate::from_ymd_opt(2026, 3, 1).unwrap(), 5),
        (NaiveDate::from_ymd_opt(2026, 3, 15).unwrap(), 3),
    ];
    state.dtako_operations = mock;

    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!(
            "{base_url}/api/operations/calendar?year=2026&month=3"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    let dates = body["dates"].as_array().unwrap();
    assert_eq!(dates.len(), 2);
    assert_eq!(dates[0]["date"], "2026-03-01");
    assert_eq!(dates[0]["count"], 5);
}

// ---------------------------------------------------------------------------
// GET /api/operations/calendar — month=12 (December, special branch)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_calendar_dates_december() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!(
            "{base_url}/api/operations/calendar?year=2026&month=12"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["year"], 2026);
    assert_eq!(body["month"], 12);
}

// ---------------------------------------------------------------------------
// GET /api/operations/calendar — invalid month=0 → 400
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_calendar_dates_invalid_month_zero() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!(
            "{base_url}/api/operations/calendar?year=2026&month=0"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

// ---------------------------------------------------------------------------
// GET /api/operations/calendar — invalid month=13 → 400
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_calendar_dates_invalid_month_13() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!(
            "{base_url}/api/operations/calendar?year=2026&month=13"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

// ---------------------------------------------------------------------------
// GET /api/operations/calendar — no auth → 401
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_calendar_dates_no_auth() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!(
            "{base_url}/api/operations/calendar?year=2026&month=3"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

// ---------------------------------------------------------------------------
// GET /api/operations/calendar — DB error → 500
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_calendar_dates_db_error() {
    let mut state = setup_mock_app_state();
    let mock = Arc::new(MockDtakoOperationsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    state.dtako_operations = mock;

    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!(
            "{base_url}/api/operations/calendar?year=2026&month=3"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// ---------------------------------------------------------------------------
// GET /api/operations/{unko_no} — success (found)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_operation_success() {
    let mut state = setup_mock_app_state();
    let mock = Arc::new(MockDtakoOperationsRepository::default());
    *mock.get_result.lock().unwrap() = vec![make_operation("1001")];
    state.dtako_operations = mock;

    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/operations/1001"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body.is_array());
    assert_eq!(body.as_array().unwrap().len(), 1);
    assert_eq!(body[0]["unko_no"], "1001");
}

// ---------------------------------------------------------------------------
// GET /api/operations/{unko_no} — not found (empty) → 404
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_operation_not_found() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/operations/9999"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

// ---------------------------------------------------------------------------
// GET /api/operations/{unko_no} — no auth → 401
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_operation_no_auth() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/operations/1001"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

// ---------------------------------------------------------------------------
// GET /api/operations/{unko_no} — DB error → 500
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_operation_db_error() {
    let mut state = setup_mock_app_state();
    let mock = Arc::new(MockDtakoOperationsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    state.dtako_operations = mock;

    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/operations/1001"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// ---------------------------------------------------------------------------
// DELETE /api/operations/{unko_no} — success (rows deleted) → 204
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_delete_operation_success() {
    let mut state = setup_mock_app_state();
    let mock = Arc::new(MockDtakoOperationsRepository::default());
    *mock.delete_rows_affected.lock().unwrap() = 1;
    state.dtako_operations = mock;

    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .delete(format!("{base_url}/api/operations/1001"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 204);
}

// ---------------------------------------------------------------------------
// DELETE /api/operations/{unko_no} — not found (0 rows) → 404
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_delete_operation_not_found() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .delete(format!("{base_url}/api/operations/9999"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

// ---------------------------------------------------------------------------
// DELETE /api/operations/{unko_no} — no auth → 401
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_delete_operation_no_auth() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .delete(format!("{base_url}/api/operations/1001"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

// ---------------------------------------------------------------------------
// DELETE /api/operations/{unko_no} — DB error → 500
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_delete_operation_db_error() {
    let mut state = setup_mock_app_state();
    let mock = Arc::new(MockDtakoOperationsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    state.dtako_operations = mock;

    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .delete(format!("{base_url}/api/operations/1001"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

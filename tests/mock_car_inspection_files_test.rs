mod common;
mod mock_helpers;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use mock_helpers::app_state::setup_mock_app_state;
use mock_helpers::MockCarInspectionRepository;

// ---------------------------------------------------------------------------
// GET /api/car-inspection-files/current — success (empty list)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_current_files_success_empty() {
    let state = setup_mock_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "MockCIFEmpty").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/car-inspection-files/current"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["files"].is_array());
    assert_eq!(body["files"].as_array().unwrap().len(), 0);
}

// ---------------------------------------------------------------------------
// GET /api/car-inspection-files/current — no auth → 401
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_current_files_no_auth() {
    let state = setup_mock_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/car-inspection-files/current"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

// ---------------------------------------------------------------------------
// GET /api/car-inspection-files/current — X-Tenant-ID header (kiosk) → 200
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_current_files_tenant_header() {
    let state = setup_mock_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "MockCIFTenant").await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/car-inspection-files/current"))
        .header("X-Tenant-ID", tenant_id.to_string())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["files"].is_array());
}

// ---------------------------------------------------------------------------
// GET /api/car-inspection-files/current — DB error (fail_next) → 500
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_current_files_db_error() {
    let mut state = setup_mock_app_state().await;

    let mock = Arc::new(MockCarInspectionRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    state.car_inspections = mock;

    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "MockCIFErr").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/car-inspection-files/current"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// ---------------------------------------------------------------------------
// GET /api/car-inspection-files/current — viewer role → 200 (no admin restriction)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_current_files_viewer_allowed() {
    let state = setup_mock_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "MockCIFViewer").await;
    let jwt = common::create_test_jwt(tenant_id, "viewer");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/car-inspection-files/current"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["files"].is_array());
}

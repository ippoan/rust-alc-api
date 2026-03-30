use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::mock_helpers::app_state::setup_mock_app_state;
use crate::mock_helpers::MockDtakoVehiclesRepository;

// ---------------------------------------------------------------------------
// GET /api/vehicles — success (empty list)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_vehicles_success() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/vehicles"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body.is_array());
    assert_eq!(body.as_array().unwrap().len(), 0);
}

// ---------------------------------------------------------------------------
// GET /api/vehicles — no auth → 401
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_vehicles_no_auth() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/vehicles"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

// ---------------------------------------------------------------------------
// GET /api/vehicles — X-Tenant-ID header (kiosk mode) → 200
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_vehicles_tenant_header() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = Uuid::new_v4();
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/vehicles"))
        .header("X-Tenant-ID", tenant_id.to_string())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body.is_array());
}

// ---------------------------------------------------------------------------
// GET /api/vehicles — DB error (fail_next) → 500
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_vehicles_db_error() {
    let mut state = setup_mock_app_state();

    // Replace dtako_vehicles with a mock that will fail on next call
    let mock = Arc::new(MockDtakoVehiclesRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    state.dtako_vehicles = mock;

    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/vehicles"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

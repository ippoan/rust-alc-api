mod common;
mod mock_helpers;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use mock_helpers::app_state::setup_mock_app_state;
use mock_helpers::MockDtakoDriversRepository;

/// GET /api/drivers — success: returns empty list (mock returns vec![])
#[tokio::test]
async fn list_drivers_success_returns_empty() {
    let state = setup_mock_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!("{base_url}/api/drivers"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Vec<serde_json::Value> = res.json().await.unwrap();
    assert!(body.is_empty());
}

/// GET /api/drivers — no auth: returns 401
#[tokio::test]
async fn list_drivers_no_auth_returns_401() {
    let state = setup_mock_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/drivers"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

/// GET /api/drivers — DB error (fail_next): returns 500
#[tokio::test]
async fn list_drivers_db_error_returns_500() {
    let mock_drivers = Arc::new(MockDtakoDriversRepository::default());
    mock_drivers.fail_next.store(true, Ordering::SeqCst);

    let mut state = setup_mock_app_state().await;
    state.dtako_drivers = mock_drivers;

    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!("{base_url}/api/drivers"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

/// GET /api/drivers — X-Tenant-ID header fallback (no JWT): returns 200
#[tokio::test]
async fn list_drivers_with_tenant_header_returns_200() {
    let state = setup_mock_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();

    let res = client
        .get(format!("{base_url}/api/drivers"))
        .header("X-Tenant-ID", tenant_id.to_string())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Vec<serde_json::Value> = res.json().await.unwrap();
    assert!(body.is_empty());
}

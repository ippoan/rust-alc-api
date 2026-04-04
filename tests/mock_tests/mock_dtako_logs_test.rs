use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::mock_helpers::app_state::setup_mock_app_state;
use crate::mock_helpers::MockDtakoLogsRepository;

// =============================================================================
// GET /api/dtako-logs/current
// =============================================================================

#[tokio::test]
async fn current_list_all_success_returns_empty() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!("{base_url}/api/dtako-logs/current"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Vec<serde_json::Value> = res.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn current_list_all_no_auth_returns_401() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/dtako-logs/current"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn current_list_all_db_error_returns_500() {
    let mock = Arc::new(MockDtakoLogsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);

    let mut state = setup_mock_app_state();
    state.dtako_logs = mock;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!("{base_url}/api/dtako-logs/current"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn current_list_all_with_tenant_header_returns_200() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();

    let res = client
        .get(format!("{base_url}/api/dtako-logs/current"))
        .header("X-Tenant-ID", tenant_id.to_string())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
}

// =============================================================================
// GET /api/dtako-logs/by-date
// =============================================================================

#[tokio::test]
async fn get_by_date_success() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!(
            "{base_url}/api/dtako-logs/by-date?date_time=26/04/04%2010:00"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Vec<serde_json::Value> = res.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn get_by_date_with_vehicle_cd() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!(
            "{base_url}/api/dtako-logs/by-date?date_time=26/04/04%2010:00&vehicle_cd=123"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn get_by_date_no_auth_returns_401() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!(
            "{base_url}/api/dtako-logs/by-date?date_time=26/04/04%2010:00"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn get_by_date_db_error_returns_500() {
    let mock = Arc::new(MockDtakoLogsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);

    let mut state = setup_mock_app_state();
    state.dtako_logs = mock;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!(
            "{base_url}/api/dtako-logs/by-date?date_time=26/04/04%2010:00"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =============================================================================
// GET /api/dtako-logs/current/select
// =============================================================================

#[tokio::test]
async fn current_list_select_success() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!("{base_url}/api/dtako-logs/current/select"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Vec<serde_json::Value> = res.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn current_list_select_with_filters() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!(
            "{base_url}/api/dtako-logs/current/select?address_disp_p=Tokyo&branch_cd=1&vehicle_cds=10,20,30"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn current_list_select_no_auth_returns_401() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/dtako-logs/current/select"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn current_list_select_db_error_returns_500() {
    let mock = Arc::new(MockDtakoLogsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);

    let mut state = setup_mock_app_state();
    state.dtako_logs = mock;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!("{base_url}/api/dtako-logs/current/select"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =============================================================================
// GET /api/dtako-logs/by-date-range
// =============================================================================

#[tokio::test]
async fn get_by_date_range_success() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!(
            "{base_url}/api/dtako-logs/by-date-range?start_date_time=2026-04-01T00:00:00Z&end_date_time=2026-04-04T23:59:59Z"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Vec<serde_json::Value> = res.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn get_by_date_range_with_vehicle_cd() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!(
            "{base_url}/api/dtako-logs/by-date-range?start_date_time=2026-04-01T00:00:00Z&end_date_time=2026-04-04T23:59:59Z&vehicle_cd=42"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn get_by_date_range_no_auth_returns_401() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!(
            "{base_url}/api/dtako-logs/by-date-range?start_date_time=2026-04-01T00:00:00Z&end_date_time=2026-04-04T23:59:59Z"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn get_by_date_range_db_error_returns_500() {
    let mock = Arc::new(MockDtakoLogsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);

    let mut state = setup_mock_app_state();
    state.dtako_logs = mock;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let token = crate::common::create_test_jwt(tenant_id, "admin");

    let res = client
        .get(format!(
            "{base_url}/api/dtako-logs/by-date-range?start_date_time=2026-04-01T00:00:00Z&end_date_time=2026-04-04T23:59:59Z"
        ))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

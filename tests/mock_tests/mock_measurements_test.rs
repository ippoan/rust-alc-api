use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::common::mock_storage::MockStorage;
use crate::mock_helpers::MockMeasurementsRepository;
use serde_json::Value;

// =========================================================================
// POST /api/measurements — success (201)
// =========================================================================

#[tokio::test]
async fn test_create_measurement_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let body = serde_json::json!({
        "employee_id": Uuid::new_v4(),
        "alcohol_value": 0.0,
        "result_type": "pass",
    });

    let res = client
        .post(format!("{base_url}/api/measurements"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&body)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 201);
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["status"], "completed");
}

// =========================================================================
// POST /api/measurements — DB error (500)
// =========================================================================

#[tokio::test]
async fn test_create_measurement_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockMeasurementsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let body = serde_json::json!({
        "employee_id": Uuid::new_v4(),
        "alcohol_value": 0.0,
        "result_type": "pass",
    });

    let res = client
        .post(format!("{base_url}/api/measurements"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&body)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// POST /api/measurements — invalid result_type (400)
// =========================================================================

#[tokio::test]
async fn test_create_measurement_invalid_result_type() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let body = serde_json::json!({
        "employee_id": Uuid::new_v4(),
        "alcohol_value": 0.0,
        "result_type": "invalid_value",
    });

    let res = client
        .post(format!("{base_url}/api/measurements"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&body)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

// =========================================================================
// POST /api/measurements — tenko_type (通常点呼の種別)
// =========================================================================

#[tokio::test]
async fn test_create_measurement_invalid_tenko_type() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockMeasurementsRepository::default());
    // repository まで届いたら 500 になる = 400 は repository より前で返っている
    mock.fail_next.store(true, Ordering::SeqCst);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock;
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let body = serde_json::json!({
        "employee_id": Uuid::new_v4(),
        "alcohol_value": 0.0,
        "result_type": "pass",
        "record_as_tenko": true,
        "tenko_type": "mid_operation",
    });

    let res = client
        .post(format!("{base_url}/api/measurements"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&body)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_create_measurement_invalid_carins_number() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockMeasurementsRepository::default());
    // repository まで届いたら 500 になる = 400 は repository より前で返っている
    mock.fail_next.store(true, Ordering::SeqCst);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock;
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let body = serde_json::json!({
        "employee_id": Uuid::new_v4(),
        "alcohol_value": 0.0,
        "result_type": "pass",
        "record_as_tenko": true,
        "carins_cert_no": "000000000001",
        "carins_vehicle_id": "TESTCARID-0001",
    });

    let res = client
        .post(format!("{base_url}/api/measurements"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&body)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_update_measurement_invalid_carins_number() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockMeasurementsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock;
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let body = serde_json::json!({
        "status": "completed",
        "record_as_tenko": true,
        "carins_cert_no": "12345",
    });

    let res = client
        .put(format!("{base_url}/api/measurements/{}", Uuid::new_v4()))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&body)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_create_measurement_valid_tenko_type() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let body = serde_json::json!({
        "employee_id": Uuid::new_v4(),
        "alcohol_value": 0.0,
        "result_type": "pass",
        "record_as_tenko": true,
        "tenko_type": "pre_operation",
    });

    let res = client
        .post(format!("{base_url}/api/measurements"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&body)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 201);
}

// =========================================================================
// POST /api/measurements — invalid JSON (400)
// =========================================================================

#[tokio::test]
async fn test_create_measurement_invalid_json() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/measurements"))
        .header("Authorization", format!("Bearer {jwt}"))
        .header("Content-Type", "application/json")
        .body("{invalid json")
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

// =========================================================================
// POST /api/measurements/start — success (201)
// =========================================================================

#[tokio::test]
async fn test_start_measurement_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let body = serde_json::json!({
        "employee_id": Uuid::new_v4(),
    });

    let res = client
        .post(format!("{base_url}/api/measurements/start"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&body)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 201);
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["status"], "started");
}

// =========================================================================
// POST /api/measurements/start — DB error (500)
// =========================================================================

#[tokio::test]
async fn test_start_measurement_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockMeasurementsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let body = serde_json::json!({
        "employee_id": Uuid::new_v4(),
    });

    let res = client
        .post(format!("{base_url}/api/measurements/start"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&body)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/measurements — success (empty list)
// =========================================================================

#[tokio::test]
async fn test_list_measurements_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/measurements"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["measurements"], serde_json::json!([]));
    assert_eq!(json["total"], 0);
    assert_eq!(json["page"], 1);
    assert_eq!(json["per_page"], 50);
}

// =========================================================================
// GET /api/measurements — DB error (500)
// =========================================================================

#[tokio::test]
async fn test_list_measurements_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockMeasurementsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/measurements"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/measurements/{id} — not found (404)
// =========================================================================

#[tokio::test]
async fn test_get_measurement_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/measurements/{id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

// =========================================================================
// GET /api/measurements/{id} — found (200)
// =========================================================================

#[tokio::test]
async fn test_get_measurement_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockMeasurementsRepository::default());
    mock.return_some.store(true, Ordering::SeqCst);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/measurements/{id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["status"], "completed");
}

// =========================================================================
// GET /api/measurements/{id} — DB error (500)
// =========================================================================

#[tokio::test]
async fn test_get_measurement_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockMeasurementsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/measurements/{id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// PUT /api/measurements/{id} — success (200)
// =========================================================================

#[tokio::test]
async fn test_update_measurement_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockMeasurementsRepository::default());
    mock.return_some.store(true, Ordering::SeqCst);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let body = serde_json::json!({
        "status": "completed",
        "alcohol_value": 0.15,
        "result_type": "fail",
    });

    let res = client
        .put(format!("{base_url}/api/measurements/{id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&body)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let json: Value = res.json().await.unwrap();
    assert!(json["id"].is_string());
}

// =========================================================================
// PUT /api/measurements/{id} — success without status field (200)
// =========================================================================

#[tokio::test]
async fn test_update_measurement_without_status_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockMeasurementsRepository::default());
    mock.return_some.store(true, Ordering::SeqCst);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    // Send without status field to skip the status validation branch
    let body = serde_json::json!({
        "alcohol_value": 0.15,
    });

    let res = client
        .put(format!("{base_url}/api/measurements/{id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&body)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let json: Value = res.json().await.unwrap();
    assert!(json["id"].is_string());
}

// =========================================================================
// PUT /api/measurements/{id} — not found (404)
// =========================================================================

#[tokio::test]
async fn test_update_measurement_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    // return_some defaults to false → update returns None → 404
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let body = serde_json::json!({
        "status": "completed",
    });

    let res = client
        .put(format!("{base_url}/api/measurements/{id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&body)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

// =========================================================================
// PUT /api/measurements/{id} — DB error (500)
// =========================================================================

#[tokio::test]
async fn test_update_measurement_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockMeasurementsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let body = serde_json::json!({
        "status": "completed",
    });

    let res = client
        .put(format!("{base_url}/api/measurements/{id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&body)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// PUT /api/measurements/{id} — invalid result_type (400)
// =========================================================================

#[tokio::test]
async fn test_update_measurement_invalid_result_type() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let body = serde_json::json!({
        "result_type": "bogus",
    });

    let res = client
        .put(format!("{base_url}/api/measurements/{id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&body)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

// =========================================================================
// PUT /api/measurements/{id} — invalid status (400)
// =========================================================================

#[tokio::test]
async fn test_update_measurement_invalid_status() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let body = serde_json::json!({
        "status": "unknown_status",
    });

    let res = client
        .put(format!("{base_url}/api/measurements/{id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&body)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

// =========================================================================
// PUT /api/measurements/{id} — tenko_type (通常点呼の種別)
// =========================================================================

#[tokio::test]
async fn test_update_measurement_invalid_tenko_type() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockMeasurementsRepository::default());
    // repository まで届いたら 500 になる = 400 は repository より前で返っている
    mock.fail_next.store(true, Ordering::SeqCst);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock;
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let body = serde_json::json!({
        "status": "completed",
        "record_as_tenko": true,
        "tenko_type": "NORMAL",
    });

    let res = client
        .put(format!("{base_url}/api/measurements/{id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&body)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_update_measurement_valid_tenko_type() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockMeasurementsRepository::default());
    mock.return_some.store(true, Ordering::SeqCst);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock;
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let body = serde_json::json!({
        "status": "completed",
        "record_as_tenko": true,
        "tenko_type": "post_operation",
    });

    let res = client
        .put(format!("{base_url}/api/measurements/{id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&body)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
}

// =========================================================================
// PUT /api/measurements/{id} — tenko_method (点呼方法) と tenko_session_id
// (Refs ippoan/alc-app#387)
// =========================================================================

#[tokio::test]
async fn test_update_measurement_invalid_tenko_method() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockMeasurementsRepository::default());
    // repository まで届いたら 500 になる = 400 は repository より前で返っている
    mock.fail_next.store(true, Ordering::SeqCst);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock.clone();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // 自動点呼・遠隔点呼は点呼の段を踏む別の流れなので、この口からは作らせない
    for method in ["自動点呼", "遠隔点呼", "", "IT", "it点呼", " IT点呼"] {
        let id = Uuid::new_v4();
        let body = serde_json::json!({
            "status": "completed",
            "record_as_tenko": true,
            "tenko_method": method,
        });

        let res = client
            .put(format!("{base_url}/api/measurements/{id}"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&body)
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 400, "tenko_method={method:?} は 400");
    }
    assert!(
        mock.update_tenko_methods.lock().unwrap().is_empty(),
        "400 は repository を呼ばない"
    );
}

#[tokio::test]
async fn test_update_measurement_tenko_method_reaches_repository() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockMeasurementsRepository::default());
    mock.return_some.store(true, Ordering::SeqCst);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock.clone();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // 未指定 (既存の client) / 通常点呼 / IT点呼
    for method in [None, Some("通常点呼"), Some("IT点呼")] {
        let id = Uuid::new_v4();
        let mut body = serde_json::json!({
            "status": "completed",
            "record_as_tenko": true,
        });
        if let Some(method) = method {
            body["tenko_method"] = Value::from(method);
        }

        let res = client
            .put(format!("{base_url}/api/measurements/{id}"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&body)
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), 200, "tenko_method={method:?} は通る");
    }
    assert_eq!(
        *mock.update_tenko_methods.lock().unwrap(),
        vec![
            None,
            Some("通常点呼".to_string()),
            Some("IT点呼".to_string())
        ],
        "repository には送られた値がそのまま渡る (未指定は None)"
    );
}

#[tokio::test]
async fn test_update_measurement_response_adds_only_tenko_session_id() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockMeasurementsRepository::default());
    mock.return_some.store(true, Ordering::SeqCst);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock.clone();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let put = |body: Value| {
        let client = client.clone();
        let url = format!("{base_url}/api/measurements/{}", Uuid::new_v4());
        let auth = format!("Bearer {jwt}");
        async move {
            let res = client
                .put(url)
                .header("Authorization", auth)
                .json(&body)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            res.json::<Value>().await.unwrap()
        }
    };

    // 点呼セッションが無い保存: 欄は在り、値は null
    let json = put(serde_json::json!({ "status": "completed" })).await;
    let mut keys: Vec<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "alcohol_value",
            "created_at",
            "device_use_count",
            "diastolic",
            "employee_id",
            "face_photo_url",
            "face_verified",
            "id",
            "measured_at",
            "medical_manual_input",
            "medical_measured_at",
            "pulse",
            "result_type",
            "status",
            "systolic",
            "temperature",
            "tenant_id",
            "tenko_session_id",
            "updated_at",
            "video_url",
        ],
        "測定の欄はそのまま (入れ子にならない)。増えるのは tenko_session_id だけ"
    );
    assert_eq!(json["status"], "completed");
    assert_eq!(json["result_type"], "pass");
    assert_eq!(json["alcohol_value"], 0.0);
    assert_eq!(json["tenant_id"], tenant_id.to_string());
    assert!(json["tenko_session_id"].is_null());

    // 点呼セッションができた保存: その id が載る
    let session_id = Uuid::new_v4();
    *mock.tenko_session_id.lock().unwrap() = Some(session_id);
    let json = put(serde_json::json!({
        "status": "completed",
        "record_as_tenko": true,
        "tenko_method": "IT点呼",
    }))
    .await;
    assert_eq!(json["tenko_session_id"], session_id.to_string());
    assert_eq!(json["status"], "completed");
}

// =========================================================================
// PUT /api/measurements/{id} — invalid JSON (400)
// =========================================================================

#[tokio::test]
async fn test_update_measurement_invalid_json() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .put(format!("{base_url}/api/measurements/{id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .header("Content-Type", "application/json")
        .body("{not valid json")
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

// =========================================================================
// GET /api/measurements/{id}/face-photo — success (storage proxy)
// =========================================================================

#[tokio::test]
async fn test_get_face_photo_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock_storage = Arc::new(MockStorage::new("test-bucket"));
    let photo_key = "tenant123/face-photo/abc.jpg";
    let photo_data = vec![0xFF, 0xD8, 0xFF, 0xE0]; // JPEG header
    let photo_url = mock_storage.insert_file(photo_key, photo_data.clone());

    let mock_repo = Arc::new(MockMeasurementsRepository::default());
    mock_repo.return_some.store(true, Ordering::SeqCst);
    *mock_repo.face_photo_url.lock().unwrap() = Some(photo_url);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock_repo;
    state.storage = mock_storage;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/measurements/{id}/face-photo"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    assert_eq!(res.headers().get("content-type").unwrap(), "image/jpeg");
    let body = res.bytes().await.unwrap();
    assert_eq!(body.as_ref(), &photo_data);
}

// =========================================================================
// GET /api/measurements/{id}/face-photo — no face_photo_url (404)
// =========================================================================

#[tokio::test]
async fn test_get_face_photo_no_url() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock_repo = Arc::new(MockMeasurementsRepository::default());
    mock_repo.return_some.store(true, Ordering::SeqCst);
    // face_photo_url is None by default

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock_repo;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/measurements/{id}/face-photo"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

// =========================================================================
// GET /api/measurements/{id}/face-photo — measurement not found (404)
// =========================================================================

#[tokio::test]
async fn test_get_face_photo_measurement_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    // return_some defaults to false → get returns None → 404
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/measurements/{id}/face-photo"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

// =========================================================================
// GET /api/measurements/{id}/face-photo — DB error (500)
// =========================================================================

#[tokio::test]
async fn test_get_face_photo_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock_repo = Arc::new(MockMeasurementsRepository::default());
    mock_repo.fail_next.store(true, Ordering::SeqCst);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock_repo;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/measurements/{id}/face-photo"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/measurements/{id}/video — success (storage proxy)
// =========================================================================

#[tokio::test]
async fn test_get_video_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock_storage = Arc::new(MockStorage::new("test-bucket"));
    let video_key = "tenant123/blow-video/abc.webm";
    let video_data = vec![0x1A, 0x45, 0xDF, 0xA3]; // WebM header
    let video_url = mock_storage.insert_file(video_key, video_data.clone());

    let mock_repo = Arc::new(MockMeasurementsRepository::default());
    mock_repo.return_some.store(true, Ordering::SeqCst);
    *mock_repo.video_url.lock().unwrap() = Some(video_url);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock_repo;
    state.storage = mock_storage;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/measurements/{id}/video"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    assert_eq!(res.headers().get("content-type").unwrap(), "video/webm");
    let body = res.bytes().await.unwrap();
    assert_eq!(body.as_ref(), &video_data);
}

// =========================================================================
// GET /api/measurements/{id}/video — no video_url (404)
// =========================================================================

#[tokio::test]
async fn test_get_video_no_url() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock_repo = Arc::new(MockMeasurementsRepository::default());
    mock_repo.return_some.store(true, Ordering::SeqCst);
    // video_url is None by default

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock_repo;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/measurements/{id}/video"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

// =========================================================================
// GET /api/measurements/{id}/video — measurement not found (404)
// =========================================================================

#[tokio::test]
async fn test_get_video_measurement_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    // return_some defaults to false → get returns None → 404
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/measurements/{id}/video"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

// =========================================================================
// GET /api/measurements/{id}/video — DB error (500)
// =========================================================================

#[tokio::test]
async fn test_get_video_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock_repo = Arc::new(MockMeasurementsRepository::default());
    mock_repo.fail_next.store(true, Ordering::SeqCst);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock_repo;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/measurements/{id}/video"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// Unauthorized — no JWT (401)
// =========================================================================

#[tokio::test]
async fn test_measurements_no_auth() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let client = reqwest::Client::new();

    // GET /api/measurements without auth
    let res = client
        .get(format!("{base_url}/api/measurements"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // POST /api/measurements without auth
    let res = client
        .post(format!("{base_url}/api/measurements"))
        .json(&serde_json::json!({"employee_id": Uuid::new_v4(), "alcohol_value": 0.0, "result_type": "pass"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // POST /api/measurements/start without auth
    let res = client
        .post(format!("{base_url}/api/measurements/start"))
        .json(&serde_json::json!({"employee_id": Uuid::new_v4()}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // GET /api/measurements/{id} without auth
    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/measurements/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // PUT /api/measurements/{id} without auth
    let res = client
        .put(format!("{base_url}/api/measurements/{id}"))
        .json(&serde_json::json!({"status": "completed"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // GET /api/measurements/{id}/face-photo without auth
    let res = client
        .get(format!("{base_url}/api/measurements/{id}/face-photo"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // GET /api/measurements/{id}/video without auth
    let res = client
        .get(format!("{base_url}/api/measurements/{id}/video"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

// =========================================================================
// GET /api/measurements/{id}/face-photo — storage download error (500)
// (face_photo_url points to a key not in storage)
// =========================================================================

#[tokio::test]
async fn test_get_face_photo_storage_download_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock_storage = Arc::new(MockStorage::new("test-bucket"));
    // URL points to a key that does NOT exist in storage
    let missing_url = "https://mock-storage/test-bucket/nonexistent/photo.jpg".to_string();

    let mock_repo = Arc::new(MockMeasurementsRepository::default());
    mock_repo.return_some.store(true, Ordering::SeqCst);
    *mock_repo.face_photo_url.lock().unwrap() = Some(missing_url);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock_repo;
    state.storage = mock_storage;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/measurements/{id}/face-photo"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/measurements/{id}/video — storage download error (500)
// =========================================================================

#[tokio::test]
async fn test_get_video_storage_download_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock_storage = Arc::new(MockStorage::new("test-bucket"));
    let missing_url = "https://mock-storage/test-bucket/nonexistent/video.webm".to_string();

    let mock_repo = Arc::new(MockMeasurementsRepository::default());
    mock_repo.return_some.store(true, Ordering::SeqCst);
    *mock_repo.video_url.lock().unwrap() = Some(missing_url);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock_repo;
    state.storage = mock_storage;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/measurements/{id}/video"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/measurements/{id}/face-photo — extract_key fails (500)
// (face_photo_url has a URL that doesn't match mock-storage prefix)
// =========================================================================

#[tokio::test]
async fn test_get_face_photo_extract_key_fails() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock_repo = Arc::new(MockMeasurementsRepository::default());
    mock_repo.return_some.store(true, Ordering::SeqCst);
    // URL that does NOT match mock-storage prefix → extract_key returns None → 500
    *mock_repo.face_photo_url.lock().unwrap() =
        Some("https://other-storage.example.com/photo.jpg".to_string());

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock_repo;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/measurements/{id}/face-photo"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/measurements/{id}/video — extract_key fails (500)
// =========================================================================

#[tokio::test]
async fn test_get_video_extract_key_fails() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock_repo = Arc::new(MockMeasurementsRepository::default());
    mock_repo.return_some.store(true, Ordering::SeqCst);
    *mock_repo.video_url.lock().unwrap() =
        Some("https://other-storage.example.com/video.webm".to_string());

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock_repo;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/measurements/{id}/video"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// X-Tenant-ID header fallback (no JWT) — success for tenant routes
// =========================================================================

#[tokio::test]
async fn test_measurements_with_tenant_header() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let client = reqwest::Client::new();

    // GET /api/measurements with X-Tenant-ID (no JWT)
    let res = client
        .get(format!("{base_url}/api/measurements"))
        .header("X-Tenant-ID", tenant_id.to_string())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let json: Value = res.json().await.unwrap();
    assert_eq!(json["measurements"], serde_json::json!([]));
}

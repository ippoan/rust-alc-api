mod common;
mod mock_helpers;

use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use common::mock_storage::MockStorage;
use mock_helpers::MockMeasurementsRepository;
use serde_json::Value;

// =========================================================================
// POST /api/measurements — success (201)
// =========================================================================

#[tokio::test]
async fn test_create_measurement_success() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let state = mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockMeasurementsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);

    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let state = mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
// POST /api/measurements — invalid JSON (422)
// =========================================================================

#[tokio::test]
async fn test_create_measurement_invalid_json() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let state = mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/measurements"))
        .header("Authorization", format!("Bearer {jwt}"))
        .header("Content-Type", "application/json")
        .body("{invalid json")
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 422);
}

// =========================================================================
// POST /api/measurements/start — success (201)
// =========================================================================

#[tokio::test]
async fn test_start_measurement_success() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let state = mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockMeasurementsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);

    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let state = mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockMeasurementsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);

    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let state = mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockMeasurementsRepository::default());
    mock.return_some.store(true, Ordering::SeqCst);

    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockMeasurementsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);

    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockMeasurementsRepository::default());
    mock.return_some.store(true, Ordering::SeqCst);

    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockMeasurementsRepository::default());
    mock.return_some.store(true, Ordering::SeqCst);

    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    // return_some defaults to false → update returns None → 404
    let state = mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockMeasurementsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);

    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let state = mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let state = mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
// PUT /api/measurements/{id} — invalid JSON (422)
// =========================================================================

#[tokio::test]
async fn test_update_measurement_invalid_json() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let state = mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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

    assert_eq!(res.status(), 422);
}

// =========================================================================
// GET /api/measurements/{id}/face-photo — success (storage proxy)
// =========================================================================

#[tokio::test]
async fn test_get_face_photo_success() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock_storage = Arc::new(MockStorage::new("test-bucket"));
    let photo_key = "tenant123/face-photo/abc.jpg";
    let photo_data = vec![0xFF, 0xD8, 0xFF, 0xE0]; // JPEG header
    let photo_url = mock_storage.insert_file(photo_key, photo_data.clone());

    let mock_repo = Arc::new(MockMeasurementsRepository::default());
    mock_repo.return_some.store(true, Ordering::SeqCst);
    *mock_repo.face_photo_url.lock().unwrap() = Some(photo_url);

    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock_repo;
    state.storage = mock_storage;

    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock_repo = Arc::new(MockMeasurementsRepository::default());
    mock_repo.return_some.store(true, Ordering::SeqCst);
    // face_photo_url is None by default

    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock_repo;

    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    // return_some defaults to false → get returns None → 404
    let state = mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock_repo = Arc::new(MockMeasurementsRepository::default());
    mock_repo.fail_next.store(true, Ordering::SeqCst);

    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock_repo;

    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock_storage = Arc::new(MockStorage::new("test-bucket"));
    let video_key = "tenant123/blow-video/abc.webm";
    let video_data = vec![0x1A, 0x45, 0xDF, 0xA3]; // WebM header
    let video_url = mock_storage.insert_file(video_key, video_data.clone());

    let mock_repo = Arc::new(MockMeasurementsRepository::default());
    mock_repo.return_some.store(true, Ordering::SeqCst);
    *mock_repo.video_url.lock().unwrap() = Some(video_url);

    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock_repo;
    state.storage = mock_storage;

    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock_repo = Arc::new(MockMeasurementsRepository::default());
    mock_repo.return_some.store(true, Ordering::SeqCst);
    // video_url is None by default

    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock_repo;

    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    // return_some defaults to false → get returns None → 404
    let state = mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock_repo = Arc::new(MockMeasurementsRepository::default());
    mock_repo.fail_next.store(true, Ordering::SeqCst);

    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock_repo;

    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let state = mock_helpers::app_state::setup_mock_app_state();
    let base_url = common::spawn_test_server(state).await;
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock_storage = Arc::new(MockStorage::new("test-bucket"));
    // URL points to a key that does NOT exist in storage
    let missing_url = "https://mock-storage/test-bucket/nonexistent/photo.jpg".to_string();

    let mock_repo = Arc::new(MockMeasurementsRepository::default());
    mock_repo.return_some.store(true, Ordering::SeqCst);
    *mock_repo.face_photo_url.lock().unwrap() = Some(missing_url);

    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock_repo;
    state.storage = mock_storage;

    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock_storage = Arc::new(MockStorage::new("test-bucket"));
    let missing_url = "https://mock-storage/test-bucket/nonexistent/video.webm".to_string();

    let mock_repo = Arc::new(MockMeasurementsRepository::default());
    mock_repo.return_some.store(true, Ordering::SeqCst);
    *mock_repo.video_url.lock().unwrap() = Some(missing_url);

    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock_repo;
    state.storage = mock_storage;

    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock_repo = Arc::new(MockMeasurementsRepository::default());
    mock_repo.return_some.store(true, Ordering::SeqCst);
    // URL that does NOT match mock-storage prefix → extract_key returns None → 500
    *mock_repo.face_photo_url.lock().unwrap() =
        Some("https://other-storage.example.com/photo.jpg".to_string());

    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock_repo;

    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock_repo = Arc::new(MockMeasurementsRepository::default());
    mock_repo.return_some.store(true, Ordering::SeqCst);
    *mock_repo.video_url.lock().unwrap() =
        Some("https://other-storage.example.com/video.webm".to_string());

    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.measurements = mock_repo;

    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let state = mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
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

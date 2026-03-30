mod common;
mod mock_helpers;

use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use mock_helpers::MockEmployeeRepository;

// ---------------------------------------------------------------------------
// Helper: spawn server with default mock state
// ---------------------------------------------------------------------------

async fn setup() -> (String, String) {
    let state = mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let base = common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

/// spawn server with a custom employees mock
async fn setup_with_mock(mock: Arc<MockEmployeeRepository>) -> (String, String) {
    let mut state = mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
    state.employees = mock;
    let base = common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

/// spawn server with fail_next=true on employees mock
async fn setup_failing() -> (String, String) {
    let mock = Arc::new(MockEmployeeRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
    state.employees = mock;
    let base = common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

// ===========================================================================
// POST /api/employees — create_employee
// ===========================================================================

#[tokio::test]
async fn create_employee_success() {
    let (base, auth) = setup().await;
    let res = client()
        .post(format!("{base}/api/employees"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "name": "Taro Yamada",
            "code": "EMP-001"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["id"].is_string());
    assert_eq!(body["name"], "Test Employee");
}

#[tokio::test]
async fn create_employee_db_error_returns_500() {
    let (base, auth) = setup_failing().await;
    let res = client()
        .post(format!("{base}/api/employees"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "name": "Will Fail"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// GET /api/employees — list_employees
// ===========================================================================

#[tokio::test]
async fn list_employees_success_empty() {
    let (base, auth) = setup().await;
    let res = client()
        .get(format!("{base}/api/employees"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Vec<serde_json::Value> = res.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn list_employees_db_error_returns_500() {
    let (base, auth) = setup_failing().await;
    let res = client()
        .get(format!("{base}/api/employees"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// GET /api/employees/{id} — get_employee
// ===========================================================================

#[tokio::test]
async fn get_employee_not_found() {
    let (base, auth) = setup().await;
    let id = uuid::Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/employees/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn get_employee_found() {
    let mock = Arc::new(MockEmployeeRepository::default());
    mock.return_some.store(true, Ordering::SeqCst);
    let (base, auth) = setup_with_mock(mock).await;
    let id = uuid::Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/employees/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["name"], "Test Employee");
}

#[tokio::test]
async fn get_employee_db_error_returns_500() {
    let mock = Arc::new(MockEmployeeRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base, auth) = setup_with_mock(mock).await;
    let id = uuid::Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/employees/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// PUT /api/employees/{id} — update_employee
// ===========================================================================

#[tokio::test]
async fn update_employee_success() {
    let mock = Arc::new(MockEmployeeRepository::default());
    mock.return_some.store(true, Ordering::SeqCst);
    let (base, auth) = setup_with_mock(mock).await;
    let id = uuid::Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/employees/{id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "name": "Updated Name"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["name"], "Test Employee");
}

#[tokio::test]
async fn update_employee_not_found() {
    // default mock returns None for update
    let (base, auth) = setup().await;
    let id = uuid::Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/employees/{id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "name": "No Such Employee"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn update_employee_db_error_returns_500() {
    let mock = Arc::new(MockEmployeeRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base, auth) = setup_with_mock(mock).await;
    let id = uuid::Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/employees/{id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "name": "Will Fail"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn update_employee_conflict_returns_409() {
    let mock = Arc::new(MockEmployeeRepository::default());
    mock.return_conflict.store(true, Ordering::SeqCst);
    let (base, auth) = setup_with_mock(mock).await;
    let id = uuid::Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/employees/{id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "name": "Duplicate Code"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 409);
}

// ===========================================================================
// DELETE /api/employees/{id} — delete_employee
// ===========================================================================

#[tokio::test]
async fn delete_employee_success() {
    let mock = Arc::new(MockEmployeeRepository::default());
    mock.return_deleted.store(true, Ordering::SeqCst);
    let (base, auth) = setup_with_mock(mock).await;
    let id = uuid::Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/employees/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn delete_employee_not_found() {
    // default mock returns false for delete
    let (base, auth) = setup().await;
    let id = uuid::Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/employees/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn delete_employee_db_error_returns_500() {
    let mock = Arc::new(MockEmployeeRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base, auth) = setup_with_mock(mock).await;
    let id = uuid::Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/employees/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// PUT /api/employees/{id}/face — update_face
// ===========================================================================

#[tokio::test]
async fn update_face_success() {
    let mock = Arc::new(MockEmployeeRepository::default());
    mock.return_some.store(true, Ordering::SeqCst);
    let (base, auth) = setup_with_mock(mock).await;
    let id = uuid::Uuid::new_v4();
    let embedding: Vec<f64> = vec![0.1; 1024];
    let res = client()
        .put(format!("{base}/api/employees/{id}/face"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "face_photo_url": "https://example.com/photo.jpg",
            "face_embedding": embedding,
            "face_model_version": "v1"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["name"], "Test Employee");
}

#[tokio::test]
async fn update_face_db_error_returns_500() {
    let mock = Arc::new(MockEmployeeRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base, auth) = setup_with_mock(mock).await;
    let id = uuid::Uuid::new_v4();
    let embedding: Vec<f64> = vec![0.1; 1024];
    let res = client()
        .put(format!("{base}/api/employees/{id}/face"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "face_photo_url": "https://example.com/photo.jpg",
            "face_embedding": embedding,
            "face_model_version": "v1"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn update_face_bad_embedding_length_returns_400() {
    let (base, auth) = setup().await;
    let id = uuid::Uuid::new_v4();
    // embedding length != 1024 should be rejected
    let embedding: Vec<f64> = vec![0.1; 512];
    let res = client()
        .put(format!("{base}/api/employees/{id}/face"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "face_photo_url": "https://example.com/photo.jpg",
            "face_embedding": embedding,
            "face_model_version": "v1"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

// ===========================================================================
// PUT /api/employees/{id}/nfc — update_nfc_id
// ===========================================================================

#[tokio::test]
async fn update_nfc_id_success() {
    let mock = Arc::new(MockEmployeeRepository::default());
    mock.return_some.store(true, Ordering::SeqCst);
    let (base, auth) = setup_with_mock(mock).await;
    let id = uuid::Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/employees/{id}/nfc"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "nfc_id": "nfc-new-123"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["name"], "Test Employee");
}

#[tokio::test]
async fn update_nfc_id_db_error_returns_500() {
    let mock = Arc::new(MockEmployeeRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base, auth) = setup_with_mock(mock).await;
    let id = uuid::Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/employees/{id}/nfc"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "nfc_id": "nfc-fail"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// PUT /api/employees/{id}/license — update_license
// ===========================================================================

#[tokio::test]
async fn update_license_success() {
    let mock = Arc::new(MockEmployeeRepository::default());
    mock.return_some.store(true, Ordering::SeqCst);
    let (base, auth) = setup_with_mock(mock).await;
    let id = uuid::Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/employees/{id}/license"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "license_issue_date": "2025-01-01",
            "license_expiry_date": "2028-01-01"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["name"], "Test Employee");
}

#[tokio::test]
async fn update_license_db_error_returns_500() {
    let mock = Arc::new(MockEmployeeRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base, auth) = setup_with_mock(mock).await;
    let id = uuid::Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/employees/{id}/license"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "license_issue_date": "2025-01-01",
            "license_expiry_date": "2028-01-01"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// GET /api/employees/face-data — list_face_data
// ===========================================================================

#[tokio::test]
async fn list_face_data_success() {
    let (base, auth) = setup().await;
    let res = client()
        .get(format!("{base}/api/employees/face-data"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Vec<serde_json::Value> = res.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn list_face_data_db_error_returns_500() {
    let (base, auth) = setup_failing().await;
    let res = client()
        .get(format!("{base}/api/employees/face-data"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// PUT /api/employees/{id}/face/approve — approve_face
// ===========================================================================

#[tokio::test]
async fn approve_face_success() {
    let mock = Arc::new(MockEmployeeRepository::default());
    mock.return_some.store(true, Ordering::SeqCst);
    let (base, auth) = setup_with_mock(mock).await;
    let id = uuid::Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/employees/{id}/face/approve"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["name"], "Test Employee");
}

#[tokio::test]
async fn approve_face_not_found() {
    let (base, auth) = setup().await;
    let id = uuid::Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/employees/{id}/face/approve"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn approve_face_db_error_returns_500() {
    let mock = Arc::new(MockEmployeeRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base, auth) = setup_with_mock(mock).await;
    let id = uuid::Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/employees/{id}/face/approve"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// PUT /api/employees/{id}/face/reject — reject_face
// ===========================================================================

#[tokio::test]
async fn reject_face_success() {
    let mock = Arc::new(MockEmployeeRepository::default());
    mock.return_some.store(true, Ordering::SeqCst);
    let (base, auth) = setup_with_mock(mock).await;
    let id = uuid::Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/employees/{id}/face/reject"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["name"], "Test Employee");
}

#[tokio::test]
async fn reject_face_not_found() {
    let (base, auth) = setup().await;
    let id = uuid::Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/employees/{id}/face/reject"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn reject_face_db_error_returns_500() {
    let mock = Arc::new(MockEmployeeRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base, auth) = setup_with_mock(mock).await;
    let id = uuid::Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/employees/{id}/face/reject"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// GET /api/employees/by-nfc/{nfc_id} — get_employee_by_nfc
// ===========================================================================

#[tokio::test]
async fn get_employee_by_nfc_found() {
    let mock = Arc::new(MockEmployeeRepository::default());
    mock.return_some.store(true, Ordering::SeqCst);
    let (base, auth) = setup_with_mock(mock).await;
    let res = client()
        .get(format!("{base}/api/employees/by-nfc/nfc-abc-123"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["name"], "Test Employee");
}

#[tokio::test]
async fn get_employee_by_nfc_not_found() {
    let (base, auth) = setup().await;
    let res = client()
        .get(format!("{base}/api/employees/by-nfc/nonexistent"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn get_employee_by_nfc_db_error_returns_500() {
    let mock = Arc::new(MockEmployeeRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base, auth) = setup_with_mock(mock).await;
    let res = client()
        .get(format!("{base}/api/employees/by-nfc/nfc-fail"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// GET /api/employees/by-code/{code} — get_employee_by_code
// ===========================================================================

#[tokio::test]
async fn get_employee_by_code_found() {
    let mock = Arc::new(MockEmployeeRepository::default());
    mock.return_some.store(true, Ordering::SeqCst);
    let (base, auth) = setup_with_mock(mock).await;
    let res = client()
        .get(format!("{base}/api/employees/by-code/EMP-001"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["name"], "Test Employee");
}

#[tokio::test]
async fn get_employee_by_code_not_found() {
    let (base, auth) = setup().await;
    let res = client()
        .get(format!("{base}/api/employees/by-code/NONEXISTENT"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn get_employee_by_code_db_error_returns_500() {
    let mock = Arc::new(MockEmployeeRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base, auth) = setup_with_mock(mock).await;
    let res = client()
        .get(format!("{base}/api/employees/by-code/EMP-FAIL"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// Unauthorized — no JWT → 401
// ===========================================================================

#[tokio::test]
async fn employees_unauthorized_without_jwt() {
    let state = mock_helpers::app_state::setup_mock_app_state();
    let base = common::spawn_test_server(state).await;

    // GET /api/employees without Authorization header
    let res = client()
        .get(format!("{base}/api/employees"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // POST /api/employees without Authorization header
    let res = client()
        .post(format!("{base}/api/employees"))
        .json(&serde_json::json!({"name": "test"}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

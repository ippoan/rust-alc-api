use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::mock_helpers::{MockDeviceRepository, MockTenkoSessionRepository};

// =========================================================================
// Helpers
// =========================================================================

/// Default setup: session returned with identity_verified + pre_operation.
async fn setup() -> (String, String, uuid::Uuid) {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenko_state = crate::mock_helpers::app_state::setup_mock_tenko_state();
    let tenant_id = uuid::Uuid::new_v4();
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header, tenant_id)
}

/// Setup with a custom MockTenkoSessionRepository.
async fn setup_with_mock(mock: Arc<MockTenkoSessionRepository>) -> (String, String, uuid::Uuid) {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let mut tenko_state = crate::mock_helpers::app_state::setup_mock_tenko_state();
    tenko_state.tenko_sessions = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header, tenant_id)
}

/// Setup with a custom MockTenkoSessionRepository + MockDeviceRepository, and an explicit
/// tenant_id (Refs ippoan/alc-app#322 — submit_medical の bp_enabled 判定分岐のカバレッジ用。
/// `MockDeviceRepository::lookup_device_tenant` は常に `Uuid::nil()` を返すため、
/// 「自テナント」を模すには `tenant_id: Uuid::nil()` を渡す)。
async fn setup_with_mock_and_devices(
    mock: Arc<MockTenkoSessionRepository>,
    devices: Arc<MockDeviceRepository>,
    tenant_id: uuid::Uuid,
) -> (String, String) {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let mut tenko_state = crate::mock_helpers::app_state::setup_mock_tenko_state();
    tenko_state.tenko_sessions = mock;
    tenko_state.devices = devices;
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header)
}

/// Setup with a custom mock + user_id for resume tests (needs AuthUser).
async fn setup_with_mock_and_user(
    mock: Arc<MockTenkoSessionRepository>,
) -> (String, String, uuid::Uuid, uuid::Uuid) {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let mut tenko_state = crate::mock_helpers::app_state::setup_mock_tenko_state();
    tenko_state.tenko_sessions = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let user_id = uuid::Uuid::new_v4();
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;
    let jwt =
        crate::common::create_test_jwt_for_user(user_id, tenant_id, "test@example.com", "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header, tenant_id, user_id)
}

/// Setup with fail_next=true for DB error tests.
async fn setup_failing() -> (String, String) {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let mut tenko_state = crate::mock_helpers::app_state::setup_mock_tenko_state();
    tenko_state.tenko_sessions = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header)
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

// =========================================================================
// POST /api/tenko/sessions/start
// =========================================================================

#[tokio::test]
async fn test_start_session_with_schedule_success() {
    let employee_id = Uuid::new_v4();
    let schedule_id = Uuid::new_v4();

    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_employee_id.lock().unwrap() = employee_id;
    *mock.schedule_employee_id.lock().unwrap() = employee_id;

    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .post(format!("{base_url}/api/tenko/sessions/start"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "schedule_id": schedule_id,
            "employee_id": employee_id,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "medical_pending");
    assert_eq!(body["tenko_type"], "pre_operation");
}

#[tokio::test]
async fn test_start_session_without_schedule_remote_mode() {
    let employee_id = Uuid::new_v4();
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_employee_id.lock().unwrap() = employee_id;

    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .post(format!("{base_url}/api/tenko/sessions/start"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "employee_id": employee_id,
            "tenko_type": "post_operation",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    // post_operation starts at identity_verified, not medical_pending
    assert_eq!(body["status"], "identity_verified");
    assert_eq!(body["tenko_type"], "post_operation");
}

#[tokio::test]
async fn test_start_session_schedule_not_found() {
    let employee_id = Uuid::new_v4();
    let schedule_id = Uuid::new_v4();

    let mock = Arc::new(MockTenkoSessionRepository::default());
    mock.return_schedule.store(false, Ordering::SeqCst);

    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .post(format!("{base_url}/api/tenko/sessions/start"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "schedule_id": schedule_id,
            "employee_id": employee_id,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_start_session_employee_mismatch() {
    let employee_id = Uuid::new_v4();
    let other_employee_id = Uuid::new_v4();
    let schedule_id = Uuid::new_v4();

    let mock = Arc::new(MockTenkoSessionRepository::default());
    // Schedule has different employee
    *mock.schedule_employee_id.lock().unwrap() = other_employee_id;

    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .post(format!("{base_url}/api/tenko/sessions/start"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "schedule_id": schedule_id,
            "employee_id": employee_id,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_start_session_db_error() {
    let (base_url, auth_header) = setup_failing().await;

    let res = client()
        .post(format!("{base_url}/api/tenko/sessions/start"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "schedule_id": Uuid::new_v4(),
            "employee_id": Uuid::new_v4(),
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/tenko/sessions/{id}
// =========================================================================

#[tokio::test]
async fn test_get_session_found() {
    let (base_url, auth_header, _) = setup().await;
    let session_id = Uuid::new_v4();

    let res = client()
        .get(format!("{base_url}/api/tenko/sessions/{session_id}"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["id"].as_str().is_some());
}

#[tokio::test]
async fn test_get_session_not_found() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    mock.return_session.store(false, Ordering::SeqCst);
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .get(format!("{base_url}/api/tenko/sessions/{}", Uuid::new_v4()))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_get_session_db_error() {
    let (base_url, auth_header) = setup_failing().await;

    let res = client()
        .get(format!("{base_url}/api/tenko/sessions/{}", Uuid::new_v4()))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/tenko/sessions — list
// =========================================================================

#[tokio::test]
async fn test_list_sessions_success() {
    let (base_url, auth_header, _) = setup().await;

    let res = client()
        .get(format!("{base_url}/api/tenko/sessions"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["sessions"].as_array().unwrap().len(), 0);
    assert_eq!(body["total"], 0);
    assert_eq!(body["page"], 1);
    assert!(body["per_page"].as_i64().unwrap() > 0);
}

#[tokio::test]
async fn test_list_sessions_db_error() {
    let (base_url, auth_header) = setup_failing().await;

    let res = client()
        .get(format!("{base_url}/api/tenko/sessions"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/tenko/dashboard
// =========================================================================

#[tokio::test]
async fn test_dashboard_success() {
    let (base_url, auth_header, _) = setup().await;

    let res = client()
        .get(format!("{base_url}/api/tenko/dashboard"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["pending_schedules"], 0);
    assert_eq!(body["active_sessions"], 0);
}

#[tokio::test]
async fn test_dashboard_db_error() {
    let (base_url, auth_header) = setup_failing().await;

    let res = client()
        .get(format!("{base_url}/api/tenko/dashboard"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// PUT /api/tenko/sessions/{id}/alcohol
// =========================================================================

#[tokio::test]
async fn test_submit_alcohol_pass_pre_operation() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "identity_verified".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/alcohol",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "alcohol_result": "pass",
            "alcohol_value": 0.0,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "instruction_pending");
}

#[tokio::test]
async fn test_submit_alcohol_pass_post_operation() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "identity_verified".to_string();
    *mock.session_tenko_type.lock().unwrap() = "post_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/alcohol",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "alcohol_result": "normal",
            "alcohol_value": 0.0,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "report_pending");
}

#[tokio::test]
async fn test_submit_alcohol_fail_cancels_session() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "identity_verified".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/alcohol",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "alcohol_result": "fail",
            "alcohol_value": 0.25,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "cancelled");
}

#[tokio::test]
async fn test_submit_alcohol_over_cancels_session() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "identity_verified".to_string();
    *mock.session_tenko_type.lock().unwrap() = "post_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/alcohol",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "alcohol_result": "over",
            "alcohol_value": 0.30,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "cancelled");
}

#[tokio::test]
async fn test_submit_alcohol_invalid_result() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "identity_verified".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/alcohol",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "alcohol_result": "invalid_value",
            "alcohol_value": 0.0,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_submit_alcohol_wrong_status() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "medical_pending".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/alcohol",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "alcohol_result": "pass",
            "alcohol_value": 0.0,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_submit_alcohol_not_found() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    mock.return_session.store(false, Ordering::SeqCst);
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/alcohol",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "alcohol_result": "pass",
            "alcohol_value": 0.0,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_submit_alcohol_db_error() {
    let (base_url, auth_header) = setup_failing().await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/alcohol",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "alcohol_result": "pass",
            "alcohol_value": 0.0,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// PUT /api/tenko/sessions/{id}/medical
// =========================================================================

#[tokio::test]
async fn test_submit_medical_success() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "medical_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/medical",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "temperature": 36.5,
            "systolic": 120,
            "diastolic": 80,
            "pulse": 72,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "self_declaration_pending");
}

#[tokio::test]
async fn test_submit_medical_wrong_tenko_type() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "medical_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "post_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/medical",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "temperature": 36.5,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
    // 本文ゼロの 400 (「API エラー (400):」しか出ない) が 2 回本番で再現した
    // (Refs yhonda-ohishi-alc/alc-app#322)。本文に理由が載ることを確かめる
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["error"], "invalid_tenko_type");
    assert!(body["message"].as_str().is_some_and(|m| !m.is_empty()));
}

#[tokio::test]
async fn test_submit_medical_wrong_status() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "identity_verified".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/medical",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "temperature": 36.5,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["error"], "invalid_session_status");
    assert!(body["message"].as_str().is_some_and(|m| !m.is_empty()));
}

#[tokio::test]
async fn test_submit_medical_not_found() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    mock.return_session.store(false, Ordering::SeqCst);
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/medical",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "temperature": 36.5,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["error"], "session_not_found");
}

#[tokio::test]
async fn test_submit_medical_db_error() {
    let (base_url, auth_header) = setup_failing().await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/medical",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "temperature": 36.5,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// --- 自動点呼のときだけ血圧必須 (Refs ippoan/alc-app-s3#135) ---

#[tokio::test]
async fn test_submit_medical_auto_missing_bp_returns_400() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "medical_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    *mock.session_tenko_method.lock().unwrap() = "自動点呼".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/medical",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "temperature": 36.5,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["error"], "bp_required");
}

#[tokio::test]
async fn test_submit_medical_remote_missing_bp_returns_200() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "medical_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    *mock.session_tenko_method.lock().unwrap() = "遠隔点呼".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/medical",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "temperature": 36.5,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_submit_medical_normal_missing_bp_returns_200() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "medical_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    *mock.session_tenko_method.lock().unwrap() = "通常点呼".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/medical",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "temperature": 36.5,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
}

// --- 血圧必須は端末の devices.bp_enabled が正本 (Refs ippoan/alc-app#322) ---

#[tokio::test]
async fn test_submit_medical_device_bp_enabled_false_returns_200() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "medical_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    *mock.session_tenko_method.lock().unwrap() = "自動点呼".to_string();

    let devices = Arc::new(MockDeviceRepository::default());
    devices.return_data.store(true, Ordering::SeqCst);
    devices.return_bp_enabled.store(false, Ordering::SeqCst);

    let (base_url, auth_header) = setup_with_mock_and_devices(mock, devices, Uuid::nil()).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/medical",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "temperature": 36.5,
            "device_id": Uuid::new_v4(),
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        200,
        "bp_enabled=false の端末は血圧なしで通るはず"
    );
}

#[tokio::test]
async fn test_submit_medical_device_bp_enabled_true_returns_400() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "medical_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    *mock.session_tenko_method.lock().unwrap() = "自動点呼".to_string();

    let devices = Arc::new(MockDeviceRepository::default());
    devices.return_data.store(true, Ordering::SeqCst);
    devices.return_bp_enabled.store(true, Ordering::SeqCst);

    let (base_url, auth_header) = setup_with_mock_and_devices(mock, devices, Uuid::nil()).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/medical",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "temperature": 36.5,
            "device_id": Uuid::new_v4(),
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        400,
        "bp_enabled=true の端末は従来どおり血圧なしを弾くはず (回帰)"
    );
}

#[tokio::test]
async fn test_submit_medical_device_cross_tenant_returns_400() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "medical_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    *mock.session_tenko_method.lock().unwrap() = "自動点呼".to_string();

    let devices = Arc::new(MockDeviceRepository::default());
    devices.return_data.store(true, Ordering::SeqCst);
    devices.return_bp_enabled.store(false, Ordering::SeqCst);

    // MockDeviceRepository::lookup_device_tenant は常に Uuid::nil() を返す。
    // リクエストの tenant を nil でない値にすると「他テナントの端末」を模せる
    let (base_url, auth_header) = setup_with_mock_and_devices(mock, devices, Uuid::new_v4()).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/medical",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "temperature": 36.5,
            "device_id": Uuid::new_v4(),
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        400,
        "device_id が別テナントの端末なら bp_enabled=false でも血圧必須のまま (なりすまし防止)"
    );
}

#[tokio::test]
async fn test_submit_medical_device_not_found_returns_400() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "medical_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    *mock.session_tenko_method.lock().unwrap() = "自動点呼".to_string();

    // return_data 既定 false = lookup_device_tenant が None を返す (device 未発見)
    let devices = Arc::new(MockDeviceRepository::default());

    let (base_url, auth_header) = setup_with_mock_and_devices(mock, devices, Uuid::nil()).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/medical",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "temperature": 36.5,
            "device_id": Uuid::new_v4(),
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        400,
        "存在しない device_id は血圧必須のまま (フェイルクローズ)"
    );
}

#[tokio::test]
async fn test_submit_medical_device_lookup_tenant_db_error_returns_500() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "medical_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    *mock.session_tenko_method.lock().unwrap() = "自動点呼".to_string();

    let devices = Arc::new(MockDeviceRepository::default());
    devices.fail_next.store(true, Ordering::SeqCst);

    let (base_url, auth_header) = setup_with_mock_and_devices(mock, devices, Uuid::nil()).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/medical",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "temperature": 36.5,
            "device_id": Uuid::new_v4(),
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_submit_medical_device_get_settings_db_error_returns_500() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "medical_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    *mock.session_tenko_method.lock().unwrap() = "自動点呼".to_string();

    let devices = Arc::new(MockDeviceRepository::default());
    // lookup_device_tenant は成功させ (テナント一致)、get_device_settings だけ落とす
    devices.return_data.store(true, Ordering::SeqCst);
    devices
        .fail_get_device_settings
        .store(true, Ordering::SeqCst);

    let (base_url, auth_header) = setup_with_mock_and_devices(mock, devices, Uuid::nil()).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/medical",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "temperature": 36.5,
            "device_id": Uuid::new_v4(),
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// --- ボンド状態は X-Device-Bp-Bonded ヘッダーが正本 (Refs ippoan/alc-app#322,
// ippoan/rust-alc-api#668)。CoreS3 経由の端末は devices テーブルに行を持たないため
// devices.bp_enabled を引けず、ヘッダーが無いと従来どおり血圧必須 (フェイルクローズ) に
// 倒れる。auth-worker (Refs ippoan/auth-worker#571) が CoreS3 (Refs
// ippoan/alc-app-s3#249) の署名を検証したうえで転送する。 ---

#[tokio::test]
async fn test_submit_medical_bp_bonded_header_0_returns_200() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "medical_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    *mock.session_tenko_method.lock().unwrap() = "自動点呼".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/medical",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .header("X-Device-Bp-Bonded", "0")
        .json(&serde_json::json!({
            "temperature": 36.5,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        200,
        "X-Device-Bp-Bonded: 0 (ボンドされていない) は血圧なしで通るはず"
    );
}

#[tokio::test]
async fn test_submit_medical_bp_bonded_header_1_returns_400() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "medical_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    *mock.session_tenko_method.lock().unwrap() = "自動点呼".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/medical",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .header("X-Device-Bp-Bonded", "1")
        .json(&serde_json::json!({
            "temperature": 36.5,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        400,
        "X-Device-Bp-Bonded: 1 (ボンドされている) は血圧必須のまま"
    );
}

#[tokio::test]
async fn test_submit_medical_bp_bonded_header_missing_fails_closed() {
    // ヘッダー自体が無い (古いファーム / CoreS3 以外の経路) = 不明 → 血圧必須のまま
    // (device_id も送らないので devices.bp_enabled 経路も引けない、二重にフェイルクローズ)
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "medical_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    *mock.session_tenko_method.lock().unwrap() = "自動点呼".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/medical",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "temperature": 36.5,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        400,
        "X-Device-Bp-Bonded ヘッダーが無いときは不明 → 血圧必須 (フェイルクローズ)"
    );
}

#[tokio::test]
async fn test_submit_medical_bp_bonded_header_overrides_device_bp_enabled() {
    // ヘッダーが正本: devices.bp_enabled=true (登録済み端末では血圧必須) でも、
    // ヘッダーが「ボンドされていない (0)」と言えばそちらを信じる
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "medical_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    *mock.session_tenko_method.lock().unwrap() = "自動点呼".to_string();

    let devices = Arc::new(MockDeviceRepository::default());
    devices.return_data.store(true, Ordering::SeqCst);
    devices.return_bp_enabled.store(true, Ordering::SeqCst);

    let (base_url, auth_header) = setup_with_mock_and_devices(mock, devices, Uuid::nil()).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/medical",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .header("X-Device-Bp-Bonded", "0")
        .json(&serde_json::json!({
            "temperature": 36.5,
            "device_id": Uuid::new_v4(),
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(
        res.status(),
        200,
        "X-Device-Bp-Bonded ヘッダーが devices.bp_enabled より優先されるはず"
    );
}

// =========================================================================
// PUT /api/tenko/sessions/{id}/escalate-remote
// =========================================================================

#[tokio::test]
async fn test_escalate_remote_success() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "medical_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    *mock.session_tenko_method.lock().unwrap() = "自動点呼".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/escalate-remote",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "reason": "血圧計が故障している",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["tenko_method"], "遠隔点呼");
    assert_eq!(body["remote_escalation_reason"], "血圧計が故障している");
    // JSON キーは escalated_to_remote_at (親の決定 — 画面側の管理者バッジがこの名前を見る)
    assert!(body["escalated_to_remote_at"].is_string());
}

#[tokio::test]
async fn test_escalate_remote_reason_too_long() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "medical_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    *mock.session_tenko_method.lock().unwrap() = "自動点呼".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let too_long = "あ".repeat(201);
    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/escalate-remote",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({ "reason": too_long }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_escalate_remote_empty_reason() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "medical_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/escalate-remote",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({ "reason": "  " }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_escalate_remote_not_found() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    mock.return_session.store(false, Ordering::SeqCst);
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/escalate-remote",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({ "reason": "測定不能" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_escalate_remote_wrong_tenko_type() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "identity_verified".to_string();
    *mock.session_tenko_type.lock().unwrap() = "post_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/escalate-remote",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({ "reason": "測定不能" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_escalate_remote_already_normal_tenko() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "medical_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    *mock.session_tenko_method.lock().unwrap() = "通常点呼".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/escalate-remote",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({ "reason": "測定不能" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_escalate_remote_already_escalated() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "medical_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    *mock.session_tenko_method.lock().unwrap() = "遠隔点呼".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/escalate-remote",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({ "reason": "測定不能" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400, "二重の切り替えは弾くはず");
}

#[tokio::test]
async fn test_escalate_remote_completed_status() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "completed".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/escalate-remote",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({ "reason": "測定不能" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_escalate_remote_get_db_error() {
    let (base_url, auth_header) = setup_failing().await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/escalate-remote",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({ "reason": "測定不能" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_escalate_remote_update_db_error() {
    let (base_url, auth_header) =
        setup_with_update_failing("medical_pending", "pre_operation").await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/escalate-remote",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({ "reason": "測定不能" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// PUT /api/tenko/sessions/{id}/instruction-confirm
// =========================================================================

#[tokio::test]
async fn test_confirm_instruction_success() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "instruction_pending".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/instruction-confirm",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "completed");
}

#[tokio::test]
async fn test_confirm_instruction_wrong_status() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "identity_verified".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/instruction-confirm",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_confirm_instruction_not_found() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    mock.return_session.store(false, Ordering::SeqCst);
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/instruction-confirm",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_confirm_instruction_db_error() {
    let (base_url, auth_header) = setup_failing().await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/instruction-confirm",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// PUT /api/tenko/sessions/{id}/report
// =========================================================================

#[tokio::test]
async fn test_submit_report_completed_no_instruction() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "report_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "post_operation".to_string();
    // return_instruction is false by default -> completed
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/report",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "vehicle_road_status": "No issues",
            "driver_alternation": "None",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "completed");
}

#[tokio::test]
async fn test_submit_report_with_instruction_pending() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "report_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "post_operation".to_string();
    mock.return_instruction.store(true, Ordering::SeqCst);
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/report",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "vehicle_road_status": "Road wet",
            "driver_alternation": "No alternation",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "instruction_pending");
}

#[tokio::test]
async fn test_submit_report_empty_fields() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "report_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "post_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/report",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "vehicle_road_status": "",
            "driver_alternation": "None",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_submit_report_wrong_tenko_type() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "report_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/report",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "vehicle_road_status": "OK",
            "driver_alternation": "None",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_submit_report_wrong_status() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "identity_verified".to_string();
    *mock.session_tenko_type.lock().unwrap() = "post_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/report",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "vehicle_road_status": "OK",
            "driver_alternation": "None",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_submit_report_not_found() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    mock.return_session.store(false, Ordering::SeqCst);
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/report",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "vehicle_road_status": "OK",
            "driver_alternation": "None",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_submit_report_db_error() {
    let (base_url, auth_header) = setup_failing().await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/report",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "vehicle_road_status": "OK",
            "driver_alternation": "None",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// PUT /api/tenko/sessions/{id}/self-declaration
// =========================================================================

#[tokio::test]
async fn test_submit_self_declaration_success() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "self_declaration_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/self-declaration",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "illness": false,
            "fatigue": false,
            "sleep_deprivation": false,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    // Safety judgment runs (no baseline → pass) → daily_inspection_pending
    assert_eq!(body["status"], "daily_inspection_pending");
}

#[tokio::test]
async fn test_submit_self_declaration_wrong_status() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "identity_verified".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/self-declaration",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "illness": false,
            "fatigue": false,
            "sleep_deprivation": false,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_submit_self_declaration_wrong_tenko_type() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "self_declaration_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "post_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/self-declaration",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "illness": false,
            "fatigue": false,
            "sleep_deprivation": false,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_submit_self_declaration_not_found() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    mock.return_session.store(false, Ordering::SeqCst);
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/self-declaration",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "illness": false,
            "fatigue": false,
            "sleep_deprivation": false,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_submit_self_declaration_db_error() {
    let (base_url, auth_header) = setup_failing().await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/self-declaration",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "illness": false,
            "fatigue": false,
            "sleep_deprivation": false,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// PUT /api/tenko/sessions/{id}/daily-inspection
// =========================================================================

#[tokio::test]
async fn test_submit_daily_inspection_all_ok() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "daily_inspection_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/daily-inspection",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "brakes": "ok",
            "tires": "ok",
            "lights": "ok",
            "steering": "ok",
            "wipers": "ok",
            "mirrors": "ok",
            "horn": "ok",
            "seatbelts": "ok",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    // No carrying items (default 0) -> identity_verified
    assert_eq!(body["status"], "identity_verified");
}

#[tokio::test]
async fn test_submit_daily_inspection_with_carrying_items() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "daily_inspection_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    *mock.carrying_items_count.lock().unwrap() = 3;
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/daily-inspection",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "brakes": "ok",
            "tires": "ok",
            "lights": "ok",
            "steering": "ok",
            "wipers": "ok",
            "mirrors": "ok",
            "horn": "ok",
            "seatbelts": "ok",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "carrying_items_pending");
}

#[tokio::test]
async fn test_submit_daily_inspection_ng_cancels() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "daily_inspection_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/daily-inspection",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "brakes": "ng",
            "tires": "ok",
            "lights": "ok",
            "steering": "ok",
            "wipers": "ok",
            "mirrors": "ok",
            "horn": "ok",
            "seatbelts": "ok",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "cancelled");
}

#[tokio::test]
async fn test_submit_daily_inspection_invalid_value() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "daily_inspection_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/daily-inspection",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "brakes": "invalid",
            "tires": "ok",
            "lights": "ok",
            "steering": "ok",
            "wipers": "ok",
            "mirrors": "ok",
            "horn": "ok",
            "seatbelts": "ok",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_submit_daily_inspection_wrong_status() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "identity_verified".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/daily-inspection",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "brakes": "ok",
            "tires": "ok",
            "lights": "ok",
            "steering": "ok",
            "wipers": "ok",
            "mirrors": "ok",
            "horn": "ok",
            "seatbelts": "ok",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_submit_daily_inspection_wrong_tenko_type() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "daily_inspection_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "post_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/daily-inspection",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "brakes": "ok",
            "tires": "ok",
            "lights": "ok",
            "steering": "ok",
            "wipers": "ok",
            "mirrors": "ok",
            "horn": "ok",
            "seatbelts": "ok",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_submit_daily_inspection_not_found() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    mock.return_session.store(false, Ordering::SeqCst);
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/daily-inspection",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "brakes": "ok",
            "tires": "ok",
            "lights": "ok",
            "steering": "ok",
            "wipers": "ok",
            "mirrors": "ok",
            "horn": "ok",
            "seatbelts": "ok",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_submit_daily_inspection_db_error() {
    let (base_url, auth_header) = setup_failing().await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/daily-inspection",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "brakes": "ok",
            "tires": "ok",
            "lights": "ok",
            "steering": "ok",
            "wipers": "ok",
            "mirrors": "ok",
            "horn": "ok",
            "seatbelts": "ok",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// PUT /api/tenko/sessions/{id}/carrying-items
// =========================================================================

#[tokio::test]
async fn test_submit_carrying_items_success() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "carrying_items_pending".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/carrying-items",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "checks": [
                {"item_id": Uuid::new_v4(), "checked": true},
                {"item_id": Uuid::new_v4(), "checked": false},
            ]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "identity_verified");
}

#[tokio::test]
async fn test_submit_carrying_items_wrong_status() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "identity_verified".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/carrying-items",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "checks": []
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_submit_carrying_items_not_found() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    mock.return_session.store(false, Ordering::SeqCst);
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/carrying-items",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "checks": []
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_submit_carrying_items_db_error() {
    let (base_url, auth_header) = setup_failing().await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/carrying-items",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "checks": []
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// POST /api/tenko/sessions/{id}/interrupt
// =========================================================================

#[tokio::test]
async fn test_interrupt_session_success() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "identity_verified".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/interrupt",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "reason": "Manager decision"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "interrupted");
}

#[tokio::test]
async fn test_interrupt_session_already_completed() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "completed".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/interrupt",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "reason": "Too late"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_interrupt_session_already_cancelled() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "cancelled".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/interrupt",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "reason": "Cancel"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_interrupt_session_already_interrupted() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "interrupted".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/interrupt",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "reason": "Dup"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_interrupt_session_not_found() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    mock.return_session.store(false, Ordering::SeqCst);
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/interrupt",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "reason": "Test"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_interrupt_session_db_error() {
    let (base_url, auth_header) = setup_failing().await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/interrupt",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "reason": "Test"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// POST /api/tenko/sessions/{id}/resume
// =========================================================================

#[tokio::test]
async fn test_resume_session_success() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "interrupted".to_string();
    let (base_url, auth_header, _, _) = setup_with_mock_and_user(mock).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/resume",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "reason": "Manager approved"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    // No daily_inspection or self_declaration -> resumes to daily_inspection_pending
    assert_eq!(body["status"], "daily_inspection_pending");
}

#[tokio::test]
async fn test_resume_session_with_daily_inspection() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "interrupted".to_string();
    mock.session_has_daily_inspection
        .store(true, Ordering::SeqCst);
    // self_declaration is None -> resumes to self_declaration_pending
    let (base_url, auth_header, _, _) = setup_with_mock_and_user(mock).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/resume",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "reason": "Retry"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "self_declaration_pending");
}

#[tokio::test]
async fn test_resume_session_with_both_inspections() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "interrupted".to_string();
    mock.session_has_daily_inspection
        .store(true, Ordering::SeqCst);
    mock.session_has_self_declaration
        .store(true, Ordering::SeqCst);
    // Both present -> resumes to daily_inspection_pending (fallback)
    let (base_url, auth_header, _, _) = setup_with_mock_and_user(mock).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/resume",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "reason": "Retry all"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "daily_inspection_pending");
}

#[tokio::test]
async fn test_resume_session_empty_reason() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "interrupted".to_string();
    let (base_url, auth_header, _, _) = setup_with_mock_and_user(mock).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/resume",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "reason": "   "
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_resume_session_wrong_status() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "identity_verified".to_string();
    let (base_url, auth_header, _, _) = setup_with_mock_and_user(mock).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/resume",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "reason": "Try resume"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_resume_session_not_found() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    mock.return_session.store(false, Ordering::SeqCst);
    let (base_url, auth_header, _, _) = setup_with_mock_and_user(mock).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/resume",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "reason": "Test"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_resume_session_db_error() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let mut tenko_state = crate::mock_helpers::app_state::setup_mock_tenko_state();
    tenko_state.tenko_sessions = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let user_id = uuid::Uuid::new_v4();
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;
    let jwt =
        crate::common::create_test_jwt_for_user(user_id, tenant_id, "test@example.com", "admin");
    let auth_header = format!("Bearer {jwt}");

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/resume",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "reason": "Test"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// POST /api/tenko/sessions/{id}/cancel
// =========================================================================

#[tokio::test]
async fn test_cancel_session_success() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "identity_verified".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/cancel",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "reason": "Operator cancelled"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "cancelled");
}

#[tokio::test]
async fn test_cancel_session_already_completed() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "completed".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/cancel",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_cancel_session_already_cancelled() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "cancelled".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/cancel",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_cancel_session_not_found() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    mock.return_session.store(false, Ordering::SeqCst);
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/cancel",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_cancel_session_db_error() {
    let (base_url, auth_header) = setup_failing().await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/cancel",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// POST /api/tenko/sessions/{id}/judgment (Refs ippoan/alc-app#315)
// =========================================================================

/// 判定した運行管理者役の mock employee (Refs ippoan/alc-app#315)。role に
/// manager/admin を含むかで判定 API のロール検査が分岐する
fn judge_employee(
    tenant_id: uuid::Uuid,
    id: uuid::Uuid,
    role: &str,
) -> rust_alc_api::db::models::Employee {
    let now = chrono::Utc::now();
    rust_alc_api::db::models::Employee {
        id,
        tenant_id,
        code: None,
        nfc_id: None,
        name: "Test Manager".to_string(),
        face_photo_url: None,
        face_embedding: None,
        face_embedding_at: None,
        face_model_version: None,
        face_approval_status: "none".to_string(),
        face_approved_by: None,
        face_approved_at: None,
        license_issue_date: None,
        license_expiry_date: None,
        role: vec![role.to_string()],
        created_at: now,
        updated_at: now,
        deleted_at: None,
    }
}

/// Setup with a custom tenko_sessions mock + a valid 運行管理者 employee
/// (driver_info.get_employee が role=manager の Employee を返す)。
async fn setup_with_mock_and_judge(
    mock: Arc<MockTenkoSessionRepository>,
) -> (String, String, uuid::Uuid, uuid::Uuid) {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let mut tenko_state = crate::mock_helpers::app_state::setup_mock_tenko_state();
    tenko_state.tenko_sessions = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let judge_id = uuid::Uuid::new_v4();
    let driver_info = Arc::new(crate::mock_helpers::MockDriverInfoRepository::default());
    *driver_info.employee.lock().unwrap() = Some(judge_employee(tenant_id, judge_id, "manager"));
    tenko_state.driver_info = driver_info;
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header, tenant_id, judge_id)
}

#[tokio::test]
async fn test_record_manager_judgment_ok_success() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "completed".to_string();
    let (base_url, auth_header, _, judge_id) = setup_with_mock_and_judge(mock.clone()).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/judgment",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({ "judgment": "ok", "judged_by_employee_id": judge_id }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    // 判定を記録しても status は変わらない (オーナー決定 1 の核)
    assert_eq!(body["status"], "completed");
    assert_eq!(body["manager_judgment"], "ok");
    let recorded = mock
        .recorded_manager_judgment
        .lock()
        .unwrap()
        .clone()
        .unwrap();
    assert_eq!(recorded.0, "ok");
    assert_eq!(recorded.1, None);
    assert_eq!(recorded.2, judge_id);
}

#[tokio::test]
async fn test_record_manager_judgment_ng_with_reason_success() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "completed".to_string();
    let (base_url, auth_header, _, judge_id) = setup_with_mock_and_judge(mock.clone()).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/judgment",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "judgment": "ng",
            "reason": "顔色が優れなかったため",
            "judged_by_employee_id": judge_id,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    // NG でも status は completed のまま (中止にしない)
    assert_eq!(body["status"], "completed");
    assert_eq!(body["manager_judgment"], "ng");
    assert_eq!(body["manager_judgment_reason"], "顔色が優れなかったため");
    let recorded = mock
        .recorded_manager_judgment
        .lock()
        .unwrap()
        .clone()
        .unwrap();
    assert_eq!(recorded.0, "ng");
    assert_eq!(recorded.1, Some("顔色が優れなかったため".to_string()));
}

#[tokio::test]
async fn test_record_manager_judgment_ng_without_reason_success() {
    // 理由は任意入力 (オーナー決定 3) — 無くても通る
    let mock = Arc::new(MockTenkoSessionRepository::default());
    let (base_url, auth_header, _, judge_id) = setup_with_mock_and_judge(mock).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/judgment",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({ "judgment": "ng", "judged_by_employee_id": judge_id }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_record_manager_judgment_admin_role_employee_allowed() {
    // employees.role が admin (兼務) でも通る (manager だけに限定しない)
    let mock = Arc::new(MockTenkoSessionRepository::default());
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let mut tenko_state = crate::mock_helpers::app_state::setup_mock_tenko_state();
    tenko_state.tenko_sessions = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let judge_id = uuid::Uuid::new_v4();
    let driver_info = Arc::new(crate::mock_helpers::MockDriverInfoRepository::default());
    *driver_info.employee.lock().unwrap() = Some(judge_employee(tenant_id, judge_id, "admin"));
    tenko_state.driver_info = driver_info;
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/judgment",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({ "judgment": "ok", "judged_by_employee_id": judge_id }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_record_manager_judgment_invalid_value_rejected() {
    // judgment のバリデーションは判定者の検証より前に行うので driver_info の
    // 設定は不要 (既定の setup_with_mock = driver_info は None を返す)
    let mock = Arc::new(MockTenkoSessionRepository::default());
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/judgment",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({ "judgment": "maybe", "judged_by_employee_id": Uuid::new_v4() }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_record_manager_judgment_forbidden_no_auth_user() {
    // ★ 決定変更の核: role では絞らず、ブラウザ利用者 (AuthUser) が居るかだけを見る。
    // AuthUser が全く無い (端末トークン相当) 呼び出しは 401 で弾く
    let mock = Arc::new(MockTenkoSessionRepository::default());
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let mut tenko_state = crate::mock_helpers::app_state::setup_mock_tenko_state();
    tenko_state.tenko_sessions = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;

    // X-Tenant-ID のみ (X-User-* 無し) = require_tenant_header は通すが AuthUser は
    // extension に入らない。テストハーネスの JWT decode 経由ではこの状態を作れないため
    // 生ヘッダーで直接叩く (tests/common/mod.rs のヘッダー変換シムを経由しない)
    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/judgment",
            Uuid::new_v4()
        ))
        .header("X-Tenant-ID", tenant_id.to_string())
        .json(&serde_json::json!({ "judgment": "ok", "judged_by_employee_id": Uuid::new_v4() }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_record_manager_judgment_forbidden_employee_not_found() {
    // 既定 (setup_with_mock) の driver_info は get_employee で None を返す
    let mock = Arc::new(MockTenkoSessionRepository::default());
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/judgment",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({ "judgment": "ok", "judged_by_employee_id": Uuid::new_v4() }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn test_record_manager_judgment_forbidden_employee_role_driver() {
    // employees.role が driver のみ (manager/admin を含まない) は 403
    let mock = Arc::new(MockTenkoSessionRepository::default());
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let mut tenko_state = crate::mock_helpers::app_state::setup_mock_tenko_state();
    tenko_state.tenko_sessions = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let judge_id = uuid::Uuid::new_v4();
    let driver_info = Arc::new(crate::mock_helpers::MockDriverInfoRepository::default());
    *driver_info.employee.lock().unwrap() = Some(judge_employee(tenant_id, judge_id, "driver"));
    tenko_state.driver_info = driver_info;
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/judgment",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({ "judgment": "ok", "judged_by_employee_id": judge_id }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn test_record_manager_judgment_employee_lookup_db_error() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let mut tenko_state = crate::mock_helpers::app_state::setup_mock_tenko_state();
    tenko_state.tenko_sessions = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let driver_info = Arc::new(crate::mock_helpers::MockDriverInfoRepository::default());
    driver_info
        .fail_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    tenko_state.driver_info = driver_info;
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/judgment",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({ "judgment": "ok", "judged_by_employee_id": Uuid::new_v4() }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_record_manager_judgment_not_found() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    mock.return_session.store(false, Ordering::SeqCst);
    let (base_url, auth_header, _, judge_id) = setup_with_mock_and_judge(mock).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/judgment",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({ "judgment": "ok", "judged_by_employee_id": judge_id }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_record_manager_judgment_update_db_error() {
    // get() は成功し、record_manager_judgment の UPDATE だけが失敗するケース
    // (line "record_manager_judgment DB error" の tracing::error! 分岐を通す)
    let mock = Arc::new(MockTenkoSessionRepository::default());
    mock.fail_on_update.store(true, Ordering::SeqCst);
    *mock.session_status.lock().unwrap() = "completed".to_string();
    let (base_url, auth_header, _, judge_id) = setup_with_mock_and_judge(mock).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/judgment",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({ "judgment": "ok", "judged_by_employee_id": judge_id }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// Unauthorized (no JWT → 401)
// =========================================================================

#[tokio::test]
async fn test_unauthorized_no_jwt() {
    let (base_url, _, _) = setup().await;

    let endpoints = vec![
        ("GET", format!("{base_url}/api/tenko/sessions")),
        ("GET", format!("{base_url}/api/tenko/dashboard")),
        (
            "GET",
            format!("{base_url}/api/tenko/sessions/{}", Uuid::new_v4()),
        ),
        ("POST", format!("{base_url}/api/tenko/sessions/start")),
        (
            "PUT",
            format!("{base_url}/api/tenko/sessions/{}/alcohol", Uuid::new_v4()),
        ),
        (
            "PUT",
            format!("{base_url}/api/tenko/sessions/{}/medical", Uuid::new_v4()),
        ),
        (
            "PUT",
            format!(
                "{base_url}/api/tenko/sessions/{}/instruction-confirm",
                Uuid::new_v4()
            ),
        ),
        (
            "PUT",
            format!("{base_url}/api/tenko/sessions/{}/report", Uuid::new_v4()),
        ),
        (
            "PUT",
            format!(
                "{base_url}/api/tenko/sessions/{}/self-declaration",
                Uuid::new_v4()
            ),
        ),
        (
            "PUT",
            format!(
                "{base_url}/api/tenko/sessions/{}/daily-inspection",
                Uuid::new_v4()
            ),
        ),
        (
            "PUT",
            format!(
                "{base_url}/api/tenko/sessions/{}/carrying-items",
                Uuid::new_v4()
            ),
        ),
        (
            "POST",
            format!("{base_url}/api/tenko/sessions/{}/interrupt", Uuid::new_v4()),
        ),
        (
            "POST",
            format!("{base_url}/api/tenko/sessions/{}/resume", Uuid::new_v4()),
        ),
        (
            "POST",
            format!("{base_url}/api/tenko/sessions/{}/cancel", Uuid::new_v4()),
        ),
    ];

    let c = client();
    for (method, url) in endpoints {
        let res = match method {
            "GET" => c.get(&url).send().await.unwrap(),
            "POST" => c
                .post(&url)
                .json(&serde_json::json!({}))
                .send()
                .await
                .unwrap(),
            "PUT" => c
                .put(&url)
                .json(&serde_json::json!({}))
                .send()
                .await
                .unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(
            res.status(),
            401,
            "Expected 401 for {method} {url}, got {}",
            res.status()
        );
    }
}

// =========================================================================
// Helper for fail_on_update tests (get succeeds, write fails)
// =========================================================================

async fn setup_with_update_failing(status: &str, tenko_type: &str) -> (String, String) {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    mock.fail_on_update.store(true, Ordering::SeqCst);
    *mock.session_status.lock().unwrap() = status.to_string();
    *mock.session_tenko_type.lock().unwrap() = tenko_type.to_string();
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let mut tenko_state = crate::mock_helpers::app_state::setup_mock_tenko_state();
    tenko_state.tenko_sessions = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header)
}

async fn setup_with_update_failing_user(status: &str, tenko_type: &str) -> (String, String) {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    mock.fail_on_update.store(true, Ordering::SeqCst);
    *mock.session_status.lock().unwrap() = status.to_string();
    *mock.session_tenko_type.lock().unwrap() = tenko_type.to_string();
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let mut tenko_state = crate::mock_helpers::app_state::setup_mock_tenko_state();
    tenko_state.tenko_sessions = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let user_id = uuid::Uuid::new_v4();
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;
    let jwt =
        crate::common::create_test_jwt_for_user(user_id, tenant_id, "test@example.com", "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header)
}

// =========================================================================
// DB error on UPDATE (second repo call) — covers tracing::error branches
// =========================================================================

#[tokio::test]
async fn test_start_session_create_session_db_error() {
    // fail_on_update: get_schedule_unconsumed succeeds, consume_schedule fails
    let mock = Arc::new(MockTenkoSessionRepository::default());
    mock.fail_on_update.store(true, Ordering::SeqCst);
    let employee_id = Uuid::new_v4();
    *mock.session_employee_id.lock().unwrap() = employee_id;
    *mock.schedule_employee_id.lock().unwrap() = employee_id;
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let mut tenko_state = crate::mock_helpers::app_state::setup_mock_tenko_state();
    tenko_state.tenko_sessions = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");

    let res = client()
        .post(format!("{base_url}/api/tenko/sessions/start"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "schedule_id": Uuid::new_v4(),
            "employee_id": employee_id,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_start_session_no_schedule_create_db_error() {
    // Without schedule_id: create_session is the first write call
    let mock = Arc::new(MockTenkoSessionRepository::default());
    mock.fail_on_update.store(true, Ordering::SeqCst);
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let mut tenko_state = crate::mock_helpers::app_state::setup_mock_tenko_state();
    tenko_state.tenko_sessions = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");

    let res = client()
        .post(format!("{base_url}/api/tenko/sessions/start"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "employee_id": Uuid::new_v4(),
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_submit_alcohol_update_db_error() {
    let (base_url, auth_header) =
        setup_with_update_failing("identity_verified", "pre_operation").await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/alcohol",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "alcohol_result": "pass",
            "alcohol_value": 0.0,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_submit_medical_update_db_error() {
    let (base_url, auth_header) =
        setup_with_update_failing("medical_pending", "pre_operation").await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/medical",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "temperature": 36.5,
            "systolic": 120,
            "diastolic": 80,
            "pulse": 72,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_confirm_instruction_update_db_error() {
    let (base_url, auth_header) =
        setup_with_update_failing("instruction_pending", "pre_operation").await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/instruction-confirm",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_submit_report_update_db_error() {
    let (base_url, auth_header) =
        setup_with_update_failing("report_pending", "post_operation").await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/report",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "vehicle_road_status": "OK",
            "driver_alternation": "None",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_cancel_session_update_db_error() {
    let (base_url, auth_header) =
        setup_with_update_failing("identity_verified", "pre_operation").await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/cancel",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "reason": "test cancel"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_submit_self_declaration_update_db_error() {
    let (base_url, auth_header) =
        setup_with_update_failing("self_declaration_pending", "pre_operation").await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/self-declaration",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "illness": false,
            "fatigue": false,
            "sleep_deprivation": false,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_submit_daily_inspection_update_db_error() {
    let (base_url, auth_header) =
        setup_with_update_failing("daily_inspection_pending", "pre_operation").await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/daily-inspection",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "brakes": "ok",
            "tires": "ok",
            "lights": "ok",
            "steering": "ok",
            "wipers": "ok",
            "mirrors": "ok",
            "horn": "ok",
            "seatbelts": "ok",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_submit_carrying_items_update_db_error() {
    let (base_url, auth_header) =
        setup_with_update_failing("carrying_items_pending", "pre_operation").await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/carrying-items",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "checks": [
                {"item_id": Uuid::new_v4(), "checked": true},
            ]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_interrupt_session_update_db_error() {
    let (base_url, auth_header) =
        setup_with_update_failing("identity_verified", "pre_operation").await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/interrupt",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "reason": "Manager"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_resume_session_update_db_error() {
    let (base_url, auth_header) =
        setup_with_update_failing_user("interrupted", "pre_operation").await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/resume",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "reason": "Retry"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// Alcohol with unknown tenko_type → 500 (line 162)
// =========================================================================

#[tokio::test]
async fn test_submit_alcohol_pass_unknown_tenko_type() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "identity_verified".to_string();
    *mock.session_tenko_type.lock().unwrap() = "unknown_type".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/alcohol",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "alcohol_result": "pass",
            "alcohol_value": 0.0,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// Safety judgment — fail via vitals exceeding baseline tolerance
// =========================================================================

#[tokio::test]
async fn test_self_declaration_safety_fail_systolic_out_of_range() {
    use rust_alc_api::db::models::EmployeeHealthBaseline;

    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "self_declaration_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    // Session has systolic=120, diastolic=80, temperature=36.5
    // Set baseline so systolic is out of tolerance: baseline=100, tolerance=5 → diff=20 > 5
    *mock.health_baseline.lock().unwrap() = Some(EmployeeHealthBaseline {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        employee_id: Uuid::new_v4(),
        baseline_systolic: 100,
        baseline_diastolic: 80,
        baseline_temperature: 36.5,
        systolic_tolerance: 5,
        diastolic_tolerance: 10,
        temperature_tolerance: 1.0,
        measurement_validity_minutes: 30,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    });
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/self-declaration",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "illness": false,
            "fatigue": false,
            "sleep_deprivation": false,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "interrupted");
    // Check safety_judgment
    let judgment = &body["safety_judgment"];
    assert_eq!(judgment["status"], "fail");
    let failed_items = judgment["failed_items"].as_array().unwrap();
    assert!(failed_items.iter().any(|v| v == "systolic"));
}

#[tokio::test]
async fn test_self_declaration_safety_fail_diastolic_out_of_range() {
    use rust_alc_api::db::models::EmployeeHealthBaseline;

    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "self_declaration_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    // Session has diastolic=80; set baseline=60, tolerance=5 → diff=20 > 5
    *mock.health_baseline.lock().unwrap() = Some(EmployeeHealthBaseline {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        employee_id: Uuid::new_v4(),
        baseline_systolic: 120,
        baseline_diastolic: 60,
        baseline_temperature: 36.5,
        systolic_tolerance: 10,
        diastolic_tolerance: 5,
        temperature_tolerance: 1.0,
        measurement_validity_minutes: 30,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    });
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/self-declaration",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "illness": false,
            "fatigue": false,
            "sleep_deprivation": false,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "interrupted");
    let judgment = &body["safety_judgment"];
    assert_eq!(judgment["status"], "fail");
    let failed_items = judgment["failed_items"].as_array().unwrap();
    assert!(failed_items.iter().any(|v| v == "diastolic"));
}

#[tokio::test]
async fn test_self_declaration_safety_fail_temperature_out_of_range() {
    use rust_alc_api::db::models::EmployeeHealthBaseline;

    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "self_declaration_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    // Session has temperature=36.5; set baseline=35.0, tolerance=0.3 → diff=1.5 > 0.3
    *mock.health_baseline.lock().unwrap() = Some(EmployeeHealthBaseline {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        employee_id: Uuid::new_v4(),
        baseline_systolic: 120,
        baseline_diastolic: 80,
        baseline_temperature: 35.0,
        systolic_tolerance: 10,
        diastolic_tolerance: 10,
        temperature_tolerance: 0.3,
        measurement_validity_minutes: 30,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    });
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/self-declaration",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "illness": false,
            "fatigue": false,
            "sleep_deprivation": false,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "interrupted");
    let judgment = &body["safety_judgment"];
    assert_eq!(judgment["status"], "fail");
    let failed_items = judgment["failed_items"].as_array().unwrap();
    assert!(failed_items.iter().any(|v| v == "temperature"));
}

// =========================================================================
// Safety judgment — fail via self_declaration flags
// =========================================================================

#[tokio::test]
async fn test_self_declaration_safety_fail_illness() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "self_declaration_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/self-declaration",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "illness": true,
            "fatigue": false,
            "sleep_deprivation": false,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "interrupted");
    let judgment = &body["safety_judgment"];
    assert_eq!(judgment["status"], "fail");
    let failed_items = judgment["failed_items"].as_array().unwrap();
    assert!(failed_items.iter().any(|v| v == "illness"));
}

#[tokio::test]
async fn test_self_declaration_safety_fail_fatigue() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "self_declaration_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/self-declaration",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "illness": false,
            "fatigue": true,
            "sleep_deprivation": false,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "interrupted");
    let judgment = &body["safety_judgment"];
    assert_eq!(judgment["status"], "fail");
    let failed_items = judgment["failed_items"].as_array().unwrap();
    assert!(failed_items.iter().any(|v| v == "fatigue"));
}

#[tokio::test]
async fn test_self_declaration_safety_fail_sleep_deprivation() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "self_declaration_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/self-declaration",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "illness": false,
            "fatigue": false,
            "sleep_deprivation": true,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "interrupted");
    let judgment = &body["safety_judgment"];
    assert_eq!(judgment["status"], "fail");
    let failed_items = judgment["failed_items"].as_array().unwrap();
    assert!(failed_items.iter().any(|v| v == "sleep_deprivation"));
}

// =========================================================================
// Safety judgment — pass with baseline (vitals within tolerance)
// =========================================================================

#[tokio::test]
async fn test_self_declaration_safety_pass_with_baseline() {
    use rust_alc_api::db::models::EmployeeHealthBaseline;

    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "self_declaration_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    // Session: systolic=120, diastolic=80, temperature=36.5
    // Baseline matches with wide tolerance
    *mock.health_baseline.lock().unwrap() = Some(EmployeeHealthBaseline {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        employee_id: Uuid::new_v4(),
        baseline_systolic: 120,
        baseline_diastolic: 80,
        baseline_temperature: 36.5,
        systolic_tolerance: 20,
        diastolic_tolerance: 20,
        temperature_tolerance: 2.0,
        measurement_validity_minutes: 30,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    });
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/self-declaration",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "illness": false,
            "fatigue": false,
            "sleep_deprivation": false,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "daily_inspection_pending");
    let judgment = &body["safety_judgment"];
    assert_eq!(judgment["status"], "pass");
    // medical_diffs should be present
    assert!(judgment["medical_diffs"].is_object());
}

// =========================================================================
// Safety judgment — update_safety_judgment DB error (line 714-716)
// =========================================================================

// =========================================================================
// create_tenko_record internal error paths (swallowed by `let _ = ...`)
// =========================================================================

#[tokio::test]
async fn test_confirm_instruction_record_creation_fails() {
    // get_employee_name fails inside create_tenko_record → error swallowed, still 200
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "instruction_pending".to_string();
    mock.fail_on_record.store(true, Ordering::SeqCst);
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/instruction-confirm",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();

    // confirm_instruction itself succeeds, create_tenko_record error is swallowed
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_cancel_session_record_creation_fails() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "identity_verified".to_string();
    mock.fail_on_record.store(true, Ordering::SeqCst);
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/cancel",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "reason": "test"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_alcohol_fail_record_creation_fails() {
    // alcohol fail → create_tenko_record called → get_employee_name fails
    // But alcohol fail ALSO calls get_employee_name for the webhook payload!
    // The webhook get_employee_name call (line 198) returns Err → returns 500.
    // So this test will actually get 500, not 200.
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "identity_verified".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    mock.fail_on_record.store(true, Ordering::SeqCst);
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/alcohol",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "alcohol_result": "fail",
            "alcohol_value": 0.25,
        }))
        .send()
        .await
        .unwrap();

    // get_employee_name fails at line 198 → 500
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_employee_name_not_found_in_record_creation() {
    // return_employee_name=false → get_employee_name returns Ok(None)
    // → .ok_or(INTERNAL_SERVER_ERROR) triggers line 502
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "instruction_pending".to_string();
    mock.return_employee_name.store(false, Ordering::SeqCst);
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/instruction-confirm",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();

    // create_tenko_record error swallowed, still 200
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_daily_inspection_ng_record_creation_fails() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "daily_inspection_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    mock.fail_on_record.store(true, Ordering::SeqCst);
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/daily-inspection",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "brakes": "ng",
            "tires": "ok",
            "lights": "ok",
            "steering": "ok",
            "wipers": "ok",
            "mirrors": "ok",
            "horn": "ok",
            "seatbelts": "ok",
        }))
        .send()
        .await
        .unwrap();

    // Daily inspection NG → cancel, but get_employee_name fails for webhook
    // Line 845-848: get_employee_name fails → 500
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_report_completed_record_creation_fails() {
    // report completed without instruction → create_tenko_record called
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "report_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "post_operation".to_string();
    mock.fail_on_record.store(true, Ordering::SeqCst);
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/report",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "vehicle_road_status": "OK",
            "driver_alternation": "None",
        }))
        .send()
        .await
        .unwrap();

    // create_tenko_record error swallowed, still 200
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_safety_judgment_fail_record_creation_fails() {
    // Safety judgment fails (illness=true) → create_tenko_record called → get_employee_name fails
    // But perform_safety_judgment also calls get_employee_name for webhook (line 725)
    // That call returns 500.
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "self_declaration_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    mock.fail_on_record.store(true, Ordering::SeqCst);
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/self-declaration",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "illness": true,
            "fatigue": false,
            "sleep_deprivation": false,
        }))
        .send()
        .await
        .unwrap();

    // perform_safety_judgment: get_employee_name fails at line 725 → 500
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_interrupt_session_employee_name_fails() {
    // interrupt → get_employee_name for webhook → fails
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "identity_verified".to_string();
    mock.fail_on_record.store(true, Ordering::SeqCst);
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/interrupt",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "reason": "Manager"
        }))
        .send()
        .await
        .unwrap();

    // get_employee_name fails at line 972 → 500
    assert_eq!(res.status(), 500);
}

// =========================================================================
// Alcohol error result variant (line ~154, "error" in valid_results)
// =========================================================================

#[tokio::test]
async fn test_submit_alcohol_error_result() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "identity_verified".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/alcohol",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "alcohol_result": "error",
            "alcohol_value": 0.0,
        }))
        .send()
        .await
        .unwrap();

    // "error" is in valid_results but is not "fail"/"over", so treated as pass
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "instruction_pending");
}

// =========================================================================
// Alcohol "normal" result for pre_operation
// =========================================================================

#[tokio::test]
async fn test_submit_alcohol_normal_pre_operation() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "identity_verified".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/alcohol",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "alcohol_result": "normal",
            "alcohol_value": 0.0,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "instruction_pending");
}

// =========================================================================
// Webhook fire_event coverage (lines 218, 383, 745, 864, 991-993)
// =========================================================================

/// Helper: setup with webhook=Some(MockWebhookService) + custom mock repo.
async fn setup_with_webhook(mock: Arc<MockTenkoSessionRepository>) -> (String, String, uuid::Uuid) {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let mut tenko_state = crate::mock_helpers::app_state::setup_mock_tenko_state();
    tenko_state.tenko_sessions = mock;
    tenko_state.webhook = Some(Arc::new(
        crate::mock_helpers::webhook::MockWebhookService::default(),
    ));
    let tenant_id = uuid::Uuid::new_v4();
    let base_url =
        crate::mock_helpers::app_state::spawn_mock_server_with_tenko(state, tenko_state.clone())
            .await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header, tenant_id)
}

/// Line 218: alcohol_detected webhook fires when alcohol_result="fail" + webhook=Some
#[tokio::test]
async fn test_webhook_alcohol_detected() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "identity_verified".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    let (base_url, auth_header, _) = setup_with_webhook(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/alcohol",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "alcohol_result": "fail",
            "alcohol_value": 0.25,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "cancelled");
}

/// Line 383: report_submitted webhook fires on any report submission
#[tokio::test]
async fn test_webhook_report_submitted() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "report_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "post_operation".to_string();
    let (base_url, auth_header, _) = setup_with_webhook(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/report",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "vehicle_road_status": "OK",
            "driver_alternation": "None",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
}

/// Line 745: tenko_interrupted webhook fires when safety judgment fails (illness=true)
#[tokio::test]
async fn test_webhook_tenko_interrupted_safety_fail() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "self_declaration_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    let (base_url, auth_header, _) = setup_with_webhook(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/self-declaration",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "illness": true,
            "fatigue": false,
            "sleep_deprivation": false,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "interrupted");
}

/// Line 864: inspection_ng webhook fires when daily inspection has NG items
#[tokio::test]
async fn test_webhook_inspection_ng() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "daily_inspection_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    let (base_url, auth_header, _) = setup_with_webhook(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/daily-inspection",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "brakes": "ng",
            "tires": "ok",
            "lights": "ok",
            "steering": "ok",
            "wipers": "ok",
            "mirrors": "ok",
            "horn": "ok",
            "seatbelts": "ok",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "cancelled");
}

/// Lines 991-993: tenko_interrupted webhook fires on interrupt_session
#[tokio::test]
async fn test_webhook_tenko_interrupted_via_interrupt() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "identity_verified".to_string();
    let (base_url, auth_header, _) = setup_with_webhook(mock).await;

    let res = client()
        .post(format!(
            "{base_url}/api/tenko/sessions/{}/interrupt",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "reason": "Manager decision"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "interrupted");
}

// =========================================================================
// DB error on specific methods (lines 527-529, 717-719, 932-934)
// =========================================================================

/// Lines 527-529: create_tenko_record repo method fails (error swallowed by `let _ =`)
#[tokio::test]
async fn test_create_tenko_record_repo_method_db_error() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "instruction_pending".to_string();
    mock.fail_on_create_record.store(true, Ordering::SeqCst);
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/instruction-confirm",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();

    // create_tenko_record error is swallowed by `let _ =`, so still 200
    assert_eq!(res.status(), 200);
}

/// Lines 717-719: update_safety_judgment DB error -> 500
#[tokio::test]
async fn test_safety_judgment_update_db_error() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "self_declaration_pending".to_string();
    *mock.session_tenko_type.lock().unwrap() = "pre_operation".to_string();
    mock.fail_on_safety_judgment.store(true, Ordering::SeqCst);
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/self-declaration",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "illness": false,
            "fatigue": false,
            "sleep_deprivation": false,
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

/// Lines 932-934: update_carrying_items DB error -> 500
#[tokio::test]
async fn test_update_carrying_items_db_error() {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    *mock.session_status.lock().unwrap() = "carrying_items_pending".to_string();
    mock.fail_on_update_carrying_items
        .store(true, Ordering::SeqCst);
    let (base_url, auth_header, _) = setup_with_mock(mock).await;

    let res = client()
        .put(format!(
            "{base_url}/api/tenko/sessions/{}/carrying-items",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "checks": [
                {"item_id": Uuid::new_v4(), "checked": true},
            ]
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

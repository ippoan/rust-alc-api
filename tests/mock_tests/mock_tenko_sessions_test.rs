use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::mock_helpers::MockTenkoSessionRepository;

// =========================================================================
// Helpers
// =========================================================================

/// Default setup: session returned with identity_verified + pre_operation.
async fn setup() -> (String, String, uuid::Uuid) {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header, tenant_id)
}

/// Setup with a custom MockTenkoSessionRepository.
async fn setup_with_mock(mock: Arc<MockTenkoSessionRepository>) -> (String, String, uuid::Uuid) {
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.tenko_sessions = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header, tenant_id)
}

/// Setup with a custom mock + user_id for resume tests (needs AuthUser).
async fn setup_with_mock_and_user(
    mock: Arc<MockTenkoSessionRepository>,
) -> (String, String, uuid::Uuid, uuid::Uuid) {
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.tenko_sessions = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let user_id = uuid::Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt =
        crate::common::create_test_jwt_for_user(user_id, tenant_id, "test@example.com", "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header, tenant_id, user_id)
}

/// Setup with fail_next=true for DB error tests.
async fn setup_failing() -> (String, String) {
    let mock = Arc::new(MockTenkoSessionRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.tenko_sessions = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
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
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.tenko_sessions = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let user_id = uuid::Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
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

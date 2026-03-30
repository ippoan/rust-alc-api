use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde_json::Value;

use crate::mock_helpers::app_state::setup_mock_app_state;
use crate::mock_helpers::MockDeviceRepository;

// ============================================================
// POST /devices/register/request — public (no auth)
// ============================================================

#[tokio::test]
async fn test_register_request_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/register/request"))
        .json(&serde_json::json!({ "device_name": "My Device" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["registration_code"].as_str().is_some());
    assert!(body["expires_at"].as_str().is_some());
}

#[tokio::test]
async fn test_register_request_no_device_name() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/register/request"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_register_request_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/register/request"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// GET /devices/register/status/{code} — public (no auth)
// ============================================================

#[tokio::test]
async fn test_registration_status_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/devices/register/status/123456"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["status"], "pending");
}

#[tokio::test]
async fn test_registration_status_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    // return_data=false -> None -> 404
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/devices/register/status/999999"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_registration_status_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/devices/register/status/123456"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// POST /devices/register/claim — public (no auth)
// ============================================================

#[tokio::test]
async fn test_claim_url_flow_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({
            "registration_code": "URLCODE",
            "phone_number": "090-1234-5678",
            "device_name": "My Phone"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], true);
    assert_eq!(body["flow_type"], "url");
    assert!(body["device_id"].as_str().is_some());
    assert!(body["tenant_id"].as_str().is_some());
}

#[tokio::test]
async fn test_claim_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    // return_data=false -> None -> error
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({
            "registration_code": "INVALID"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], false);
}

#[tokio::test]
async fn test_claim_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({
            "registration_code": "DBFAIL"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], false);
}

// ============================================================
// GET /devices/settings/{device_id} — public (no auth)
// ============================================================

#[tokio::test]
async fn test_device_settings_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let device_id = Uuid::new_v4();
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/devices/settings/{device_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["call_enabled"], true);
    assert_eq!(body["status"], "active");
    assert_eq!(body["always_on"], false);
}

#[tokio::test]
async fn test_device_settings_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let device_id = Uuid::new_v4();
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/devices/settings/{device_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// ============================================================
// PUT /devices/register-fcm-token — public (no auth)
// ============================================================

#[tokio::test]
async fn test_register_fcm_token_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst); // lookup_device_tenant returns Some
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .put(format!("{base_url}/api/devices/register-fcm-token"))
        .json(&serde_json::json!({
            "device_id": Uuid::new_v4(),
            "fcm_token": "new-fcm-token-xyz"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_register_fcm_token_device_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    // return_data=false -> lookup_device_tenant returns None -> 404
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .put(format!("{base_url}/api/devices/register-fcm-token"))
        .json(&serde_json::json!({
            "device_id": Uuid::new_v4(),
            "fcm_token": "token"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// ============================================================
// PUT /devices/report-version — public (no auth)
// ============================================================

#[tokio::test]
async fn test_report_version_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .put(format!("{base_url}/api/devices/report-version"))
        .json(&serde_json::json!({
            "device_id": Uuid::new_v4(),
            "version_code": 42,
            "version_name": "1.2.3",
            "is_device_owner": true,
            "is_dev_device": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_report_version_device_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .put(format!("{base_url}/api/devices/report-version"))
        .json(&serde_json::json!({
            "device_id": Uuid::new_v4(),
            "version_code": 1,
            "version_name": "0.0.1"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// ============================================================
// PUT /devices/report-watchdog — public (no auth)
// ============================================================

#[tokio::test]
async fn test_report_watchdog_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .put(format!("{base_url}/api/devices/report-watchdog"))
        .json(&serde_json::json!({
            "device_id": Uuid::new_v4(),
            "running": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_report_watchdog_device_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .put(format!("{base_url}/api/devices/report-watchdog"))
        .json(&serde_json::json!({
            "device_id": Uuid::new_v4(),
            "running": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// ============================================================
// PUT /devices/update-last-login — public (no auth)
// ============================================================

#[tokio::test]
async fn test_update_last_login_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .put(format!("{base_url}/api/devices/update-last-login"))
        .json(&serde_json::json!({
            "device_id": Uuid::new_v4(),
            "employee_id": Uuid::new_v4(),
            "employee_name": "Taro",
            "employee_role": ["driver"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

// ============================================================
// POST /devices/fcm-notify-call — public (internal)
// ============================================================

#[tokio::test]
async fn test_fcm_notify_call_no_fcm() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::remove_var("FCM_INTERNAL_SECRET");

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    // fcm = None
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/fcm-notify-call"))
        .json(&serde_json::json!({
            "room_ids": ["room-1"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 503);
}

#[tokio::test]
async fn test_fcm_notify_call_empty_rooms() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::remove_var("FCM_INTERNAL_SECRET");

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/fcm-notify-call"))
        .json(&serde_json::json!({
            "room_ids": []
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["sent"], 0);
    assert_eq!(body["skipped"], 0);
    assert_eq!(body["errors"], 0);
}

#[tokio::test]
async fn test_fcm_notify_call_with_devices() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::remove_var("FCM_INTERNAL_SECRET");

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/fcm-notify-call"))
        .json(&serde_json::json!({
            "room_ids": ["room-1"],
            "exclude_device_ids": []
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["sent"], 1);
}

#[tokio::test]
async fn test_fcm_notify_call_with_internal_secret_check() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("FCM_INTERNAL_SECRET", "my-secret-123");

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();

    // Wrong secret -> 401
    let res = client
        .post(format!("{base_url}/api/devices/fcm-notify-call"))
        .header("X-Internal-Secret", "wrong-secret")
        .json(&serde_json::json!({ "room_ids": ["room-1"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // No header -> 401
    let res = client
        .post(format!("{base_url}/api/devices/fcm-notify-call"))
        .json(&serde_json::json!({ "room_ids": ["room-1"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    std::env::remove_var("FCM_INTERNAL_SECRET");
}

// ============================================================
// POST /devices/fcm-dismiss-test — public (no auth)
// ============================================================

#[tokio::test]
async fn test_fcm_dismiss_test_no_fcm() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    // fcm = None
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/fcm-dismiss-test"))
        .json(&serde_json::json!({ "device_id": Uuid::new_v4() }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 503);
}

#[tokio::test]
async fn test_fcm_dismiss_test_device_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    // return_data=false -> get_device_tenant_active returns None -> 404
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/fcm-dismiss-test"))
        .json(&serde_json::json!({ "device_id": Uuid::new_v4() }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_fcm_dismiss_test_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/fcm-dismiss-test"))
        .json(&serde_json::json!({ "device_id": Uuid::new_v4() }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["sent"].as_u64().is_some());
}

// ============================================================
// POST /devices/test-fcm-all-exclude — public
// ============================================================

#[tokio::test]
async fn test_fcm_all_exclude_no_fcm() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/test-fcm-all-exclude"))
        .json(&serde_json::json!({ "exclude_device_ids": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 503);
}

// ============================================================
// Tenant endpoints — auth required
// ============================================================

// GET /devices — list devices
#[tokio::test]
async fn test_list_devices_empty() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/devices"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_list_devices_no_auth() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/devices"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_list_devices_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/devices"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// GET /devices/pending — list pending registrations
#[tokio::test]
async fn test_list_pending_empty() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/devices/pending"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body.as_array().unwrap().len(), 0);
}

// POST /devices/register/create-token
#[tokio::test]
async fn test_create_url_token_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/register/create-token"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "device_name": "URL Device" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["registration_code"].as_str().is_some());
    assert!(body["registration_url"]
        .as_str()
        .unwrap()
        .contains("/device-claim?token="));
}

#[tokio::test]
async fn test_create_url_token_no_auth() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/register/create-token"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

// POST /devices/register/create-permanent-qr
#[tokio::test]
async fn test_create_permanent_qr_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!(
            "{base_url}/api/devices/register/create-permanent-qr"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "device_name": "QR Device" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["registration_code"].as_str().is_some());
}

// POST /devices/register/create-device-owner-token
#[tokio::test]
async fn test_create_device_owner_token_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!(
            "{base_url}/api/devices/register/create-device-owner-token"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "device_name": "DO Device" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["registration_code"].as_str().is_some());
}

// POST /devices/approve/{id}
#[tokio::test]
async fn test_approve_device_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let req_id = Uuid::new_v4();

    let res = client
        .post(format!("{base_url}/api/devices/approve/{req_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "device_name": "Approved Device" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], true);
    assert!(body["device_id"].as_str().is_some());
}

#[tokio::test]
async fn test_approve_device_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    // return_data=false -> find_approve_request returns None -> 404
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let req_id = Uuid::new_v4();

    let res = client
        .post(format!("{base_url}/api/devices/approve/{req_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// POST /devices/approve-by-code/{code}
#[tokio::test]
async fn test_approve_by_code_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/approve-by-code/ABC123"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], true);
}

#[tokio::test]
async fn test_approve_by_code_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/approve-by-code/INVALID"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// POST /devices/reject/{id}
#[tokio::test]
async fn test_reject_device_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .post(format!("{base_url}/api/devices/reject/{id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_reject_device_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .post(format!("{base_url}/api/devices/reject/{id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// POST /devices/disable/{id}
#[tokio::test]
async fn test_disable_device_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .post(format!("{base_url}/api/devices/disable/{id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

// POST /devices/enable/{id}
#[tokio::test]
async fn test_enable_device_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .post(format!("{base_url}/api/devices/enable/{id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

// DELETE /devices/{id}
#[tokio::test]
async fn test_delete_device_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .delete(format!("{base_url}/api/devices/{id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_delete_device_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .delete(format!("{base_url}/api/devices/{id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// PUT /devices/{id}/call-settings
#[tokio::test]
async fn test_update_call_settings_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .put(format!("{base_url}/api/devices/{id}/call-settings"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({
            "call_enabled": true,
            "call_schedule": { "enabled": true, "startHour": 8, "endHour": 18 },
            "always_on": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_update_call_settings_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .put(format!("{base_url}/api/devices/{id}/call-settings"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({
            "call_enabled": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// POST /devices/{id}/test-fcm — single device FCM test
#[tokio::test]
async fn test_fcm_single_device_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .post(format!("{base_url}/api/devices/{id}/test-fcm"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], true);
}

#[tokio::test]
async fn test_fcm_single_device_no_fcm() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    // fcm = None
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .post(format!("{base_url}/api/devices/{id}/test-fcm"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 503);
}

#[tokio::test]
async fn test_fcm_single_device_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    // return_data=false -> get_device_fcm_token returns None -> 404
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .post(format!("{base_url}/api/devices/{id}/test-fcm"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_fcm_single_device_failing_sender() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::FailingFcmSender));
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .post(format!("{base_url}/api/devices/{id}/test-fcm"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(body["error"].as_str().is_some());
}

// POST /devices/test-fcm-all — tenant-wide FCM test
#[tokio::test]
async fn test_fcm_all_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/test-fcm-all"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["sent"], 1);
    assert_eq!(body["errors"], 0);
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["success"], true);
}

#[tokio::test]
async fn test_fcm_all_no_fcm() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/test-fcm-all"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 503);
}

// POST /devices/trigger-update — tenant OTA
#[tokio::test]
async fn test_trigger_update_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/trigger-update"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({
            "version_code": 99,
            "version_name": "9.9.9"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    // device has version_code=10, target=99, so should send
    assert_eq!(body["sent"], 1);
}

#[tokio::test]
async fn test_trigger_update_already_updated() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/trigger-update"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({
            "version_code": 5,
            "version_name": "0.5.0"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    // device has version_code=10 >= target=5, so already_updated
    assert_eq!(body["already_updated"], 1);
    assert_eq!(body["sent"], 0);
}

// POST /devices/trigger-update-dev — internal OTA for dev
#[tokio::test]
async fn test_trigger_update_dev_no_secret() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::remove_var("FCM_INTERNAL_SECRET");

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/trigger-update-dev"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    // FCM_INTERNAL_SECRET not set -> 503
    assert_eq!(res.status(), 503);
}

#[tokio::test]
async fn test_trigger_update_dev_wrong_secret() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("FCM_INTERNAL_SECRET", "correct-secret");

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/trigger-update-dev"))
        .header("X-Internal-Secret", "wrong-secret")
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
    std::env::remove_var("FCM_INTERNAL_SECRET");
}

#[tokio::test]
async fn test_trigger_update_dev_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("FCM_INTERNAL_SECRET", "dev-secret");

    let mock = Arc::new(MockDeviceRepository::default());
    // return_data=false -> list_dev_device_tenant_ids returns empty vec -> no devices
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/trigger-update-dev"))
        .header("X-Internal-Secret", "dev-secret")
        .json(&serde_json::json!({ "version_code": 100 }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["sent"], 0);
    std::env::remove_var("FCM_INTERNAL_SECRET");
}

// ============================================================
// X-Tenant-ID header for tenant endpoints
// ============================================================

#[tokio::test]
async fn test_list_devices_with_x_tenant_id() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/devices"))
        .header("X-Tenant-ID", Uuid::new_v4().to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// DeviceRow → Device From trait (line 122-149)
// ============================================================

#[tokio::test]
async fn test_list_devices_with_device_rows() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_device_rows.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/devices"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["device_name"], "Mock Device");
    assert_eq!(arr[0]["device_type"], "android");
    assert_eq!(arr[0]["status"], "active");
}

// ============================================================
// RegistrationRequestRow → RegistrationRequest From trait (line 169-184)
// ============================================================

#[tokio::test]
async fn test_list_pending_with_rows() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_pending_rows.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/devices/pending"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["device_name"], "Pending Device");
    assert_eq!(arr[0]["flow_type"], "qr_permanent");
}

// ============================================================
// check_registration_status: expired status (line 256)
// ============================================================

#[tokio::test]
async fn test_registration_status_expired() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    mock.return_expired.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/devices/register/status/123456"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["status"], "expired");
}

// ============================================================
// check_registration_status: non-pending status (line 264)
// ============================================================

#[tokio::test]
async fn test_registration_status_approved() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    mock.return_approved_status.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/devices/register/status/123456"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["status"], "approved");
}

// ============================================================
// check_registration_status: pending with no expires_at (line 261)
// ============================================================

#[tokio::test]
async fn test_registration_status_pending_no_expires() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    mock.return_no_expires.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/devices/register/status/123456"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["status"], "pending");
}

// ============================================================
// claim_registration: qr_permanent flow (line 367-383)
// ============================================================

#[tokio::test]
async fn test_claim_qr_permanent_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    mock.return_qr_permanent.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({
            "registration_code": "QRCODE",
            "phone_number": "090-1234-5678",
            "device_name": "QR Phone"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], true);
    assert_eq!(body["flow_type"], "qr_permanent");
    assert!(body["message"].as_str().is_some());
}

// ============================================================
// claim_registration: status != "pending" (line 326)
// ============================================================

#[tokio::test]
async fn test_claim_used_status() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    mock.return_used_status.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({
            "registration_code": "USED_CODE"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], false);
}

// ============================================================
// claim_registration: unknown flow_type (line 383)
// ============================================================

#[tokio::test]
async fn test_claim_unknown_flow_type() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    mock.return_unknown_flow.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({
            "registration_code": "UNKNOWN"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], false);
}

// ============================================================
// DB error paths for tenant endpoints
// ============================================================

#[tokio::test]
async fn test_list_pending_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/devices/pending"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_create_url_token_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/register/create-token"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_create_device_owner_token_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!(
            "{base_url}/api/devices/register/create-device-owner-token"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_create_permanent_qr_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!(
            "{base_url}/api/devices/register/create-permanent-qr"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_approve_device_db_error_lookup() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let req_id = Uuid::new_v4();

    let res = client
        .post(format!("{base_url}/api/devices/approve/{req_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_approve_by_code_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/approve-by-code/ABC"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_reject_device_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .post(format!("{base_url}/api/devices/reject/{id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_disable_device_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .post(format!("{base_url}/api/devices/disable/{id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_disable_device_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .post(format!("{base_url}/api/devices/disable/{id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_enable_device_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .post(format!("{base_url}/api/devices/enable/{id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_enable_device_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .post(format!("{base_url}/api/devices/enable/{id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_delete_device_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .delete(format!("{base_url}/api/devices/{id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_update_call_settings_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .put(format!("{base_url}/api/devices/{id}/call-settings"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "call_enabled": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// update_call_settings: always_on with no FCM token (line 857)
// ============================================================

#[tokio::test]
async fn test_update_call_settings_always_on_no_token() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    mock.return_no_fcm_token.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .put(format!("{base_url}/api/devices/{id}/call-settings"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({
            "call_enabled": true,
            "always_on": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

// ============================================================
// update_call_settings: always_on without FCM (no fcm provider)
// ============================================================

#[tokio::test]
async fn test_update_call_settings_always_on_no_fcm_provider() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    // fcm = None, so always_on FCM branch is skipped
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .put(format!("{base_url}/api/devices/{id}/call-settings"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({
            "call_enabled": true,
            "always_on": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

// ============================================================
// update_call_settings: always_on=None (no FCM sent)
// ============================================================

#[tokio::test]
async fn test_update_call_settings_no_always_on() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .put(format!("{base_url}/api/devices/{id}/call-settings"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "call_enabled": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

// ============================================================
// update_call_settings: FCM send failure (line 864)
// ============================================================

#[tokio::test]
async fn test_update_call_settings_fcm_send_failure() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::FailingFcmSender));
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .put(format!("{base_url}/api/devices/{id}/call-settings"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({
            "call_enabled": true,
            "always_on": true
        }))
        .send()
        .await
        .unwrap();
    // FCM failure is logged but doesn't cause 500
    assert_eq!(res.status(), 204);
}

// ============================================================
// Device settings DB error
// ============================================================

#[tokio::test]
async fn test_device_settings_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let device_id = Uuid::new_v4();
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/devices/settings/{device_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// FCM token registration DB errors
// ============================================================

#[tokio::test]
async fn test_register_fcm_token_db_error_lookup() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .put(format!("{base_url}/api/devices/register-fcm-token"))
        .json(&serde_json::json!({
            "device_id": Uuid::new_v4(),
            "fcm_token": "token"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// update_last_login DB errors + not found
// ============================================================

#[tokio::test]
async fn test_update_last_login_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    // return_data=false -> lookup returns None -> 404
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .put(format!("{base_url}/api/devices/update-last-login"))
        .json(&serde_json::json!({
            "device_id": Uuid::new_v4(),
            "employee_id": Uuid::new_v4(),
            "employee_name": "Taro",
            "employee_role": ["driver"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_update_last_login_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .put(format!("{base_url}/api/devices/update-last-login"))
        .json(&serde_json::json!({
            "device_id": Uuid::new_v4(),
            "employee_id": Uuid::new_v4(),
            "employee_name": "Taro",
            "employee_role": ["driver"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// report_version DB error
// ============================================================

#[tokio::test]
async fn test_report_version_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .put(format!("{base_url}/api/devices/report-version"))
        .json(&serde_json::json!({
            "device_id": Uuid::new_v4(),
            "version_code": 1,
            "version_name": "0.0.1"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// report_watchdog DB error
// ============================================================

#[tokio::test]
async fn test_report_watchdog_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .put(format!("{base_url}/api/devices/report-watchdog"))
        .json(&serde_json::json!({
            "device_id": Uuid::new_v4(),
            "running": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// should_notify_device: call_enabled=false
// ============================================================

#[tokio::test]
async fn test_fcm_notify_call_disabled_device_skipped() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::remove_var("FCM_INTERNAL_SECRET");

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    mock.return_call_disabled.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/fcm-notify-call"))
        .json(&serde_json::json!({ "room_ids": ["room-1"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["sent"], 0);
    assert_eq!(body["skipped"], 1);
}

// ============================================================
// should_notify_device: schedule enabled=false
// ============================================================

#[tokio::test]
async fn test_fcm_notify_call_schedule_disabled_skipped() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::remove_var("FCM_INTERNAL_SECRET");

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    mock.return_schedule_disabled.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/fcm-notify-call"))
        .json(&serde_json::json!({ "room_ids": ["room-1"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["sent"], 0);
    assert_eq!(body["skipped"], 1);
}

// ============================================================
// should_notify_device: schedule with days + time (all days, all hours)
// ============================================================

#[tokio::test]
async fn test_fcm_notify_call_schedule_with_days_sent() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::remove_var("FCM_INTERNAL_SECRET");

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    mock.return_schedule_with_days.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/fcm-notify-call"))
        .json(&serde_json::json!({ "room_ids": ["room-1"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["sent"], 1);
}

// ============================================================
// fcm_notify_call: FCM send error
// ============================================================

#[tokio::test]
async fn test_fcm_notify_call_fcm_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::remove_var("FCM_INTERNAL_SECRET");

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::FailingFcmSender));
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/fcm-notify-call"))
        .json(&serde_json::json!({ "room_ids": ["room-1"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["errors"], 1);
    assert_eq!(body["sent"], 0);
}

// ============================================================
// fcm_notify_call: DB error on list_fcm_devices
// ============================================================

#[tokio::test]
async fn test_fcm_notify_call_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::remove_var("FCM_INTERNAL_SECRET");

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/fcm-notify-call"))
        .json(&serde_json::json!({ "room_ids": ["room-1"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// fcm_notify_call: exclude_device_ids
// ============================================================

#[tokio::test]
async fn test_fcm_notify_call_with_exclude() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::remove_var("FCM_INTERNAL_SECRET");

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    // Exclude the nil UUID device
    let res = client
        .post(format!("{base_url}/api/devices/fcm-notify-call"))
        .json(&serde_json::json!({
            "room_ids": ["room-1"],
            "exclude_device_ids": [Uuid::nil().to_string()]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["skipped"], 1);
    assert_eq!(body["sent"], 0);
}

// ============================================================
// fcm_dismiss_test: DB error
// ============================================================

#[tokio::test]
async fn test_fcm_dismiss_test_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/fcm-dismiss-test"))
        .json(&serde_json::json!({ "device_id": Uuid::new_v4() }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// test_fcm_all_exclude: with devices
// ============================================================

#[tokio::test]
async fn test_fcm_all_exclude_with_devices() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_callable_devices.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/test-fcm-all-exclude"))
        .json(&serde_json::json!({ "exclude_device_ids": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["sent"], 1);
}

// ============================================================
// test_fcm_all_exclude: with exclude matching
// ============================================================

#[tokio::test]
async fn test_fcm_all_exclude_with_exclude_matching() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_callable_devices.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/test-fcm-all-exclude"))
        .json(&serde_json::json!({ "exclude_device_ids": [Uuid::nil().to_string()] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["sent"], 0);
}

// ============================================================
// test_fcm_all_exclude: FCM error
// ============================================================

#[tokio::test]
async fn test_fcm_all_exclude_fcm_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_callable_devices.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::FailingFcmSender));
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/test-fcm-all-exclude"))
        .json(&serde_json::json!({ "exclude_device_ids": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["errors"], 1);
    assert_eq!(body["sent"], 0);
}

// ============================================================
// test_fcm_all_exclude: DB error
// ============================================================

#[tokio::test]
async fn test_fcm_all_exclude_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/test-fcm-all-exclude"))
        .json(&serde_json::json!({ "exclude_device_ids": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// test_fcm: device has no FCM token (null token)
// ============================================================

#[tokio::test]
async fn test_fcm_single_device_null_token() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    mock.return_null_fcm_token.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .post(format!("{base_url}/api/devices/{id}/test-fcm"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

// ============================================================
// test_fcm: DB error
// ============================================================

#[tokio::test]
async fn test_fcm_single_device_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let id = Uuid::new_v4();

    let res = client
        .post(format!("{base_url}/api/devices/{id}/test-fcm"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// test_fcm_all: with FailingFcmSender
// ============================================================

#[tokio::test]
async fn test_fcm_all_failing_sender() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::FailingFcmSender));
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/test-fcm-all"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["errors"], 1);
    assert_eq!(body["sent"], 0);
    let results = body["results"].as_array().unwrap();
    assert_eq!(results[0]["success"], false);
    assert!(results[0]["error"].as_str().is_some());
}

// ============================================================
// test_fcm_all: DB error
// ============================================================

#[tokio::test]
async fn test_fcm_all_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/test-fcm-all"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// trigger_update: no FCM configured
// ============================================================

#[tokio::test]
async fn test_trigger_update_no_fcm() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    // fcm = None
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/trigger-update"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "version_code": 99 }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 503);
}

// ============================================================
// trigger_update: with device_ids filter
// ============================================================

#[tokio::test]
async fn test_trigger_update_with_device_ids_filter() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // Filter to a non-existing device ID -> skipped
    let res = client
        .post(format!("{base_url}/api/devices/trigger-update"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({
            "device_ids": [Uuid::new_v4()],
            "version_code": 99,
            "version_name": "9.9.9"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["skipped"], 1);
    assert_eq!(body["sent"], 0);
}

// ============================================================
// trigger_update: with FailingFcmSender
// ============================================================

#[tokio::test]
async fn test_trigger_update_fcm_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::FailingFcmSender));
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/trigger-update"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({
            "version_code": 99,
            "version_name": "9.9.9"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["errors"], 1);
    assert_eq!(body["sent"], 0);
}

// ============================================================
// trigger_update: DB error
// ============================================================

#[tokio::test]
async fn test_trigger_update_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/trigger-update"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "version_code": 99 }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// trigger_update: no version_code, device has no version
// ============================================================

#[tokio::test]
async fn test_trigger_update_no_version_code() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    mock.return_ota_no_version.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // No version_code in body + device has None -> skip version check, send update
    let res = client
        .post(format!("{base_url}/api/devices/trigger-update"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["sent"], 1);
}

// ============================================================
// trigger_update: with version_name only
// ============================================================

#[tokio::test]
async fn test_trigger_update_with_version_name_only() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/trigger-update"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "version_name": "1.0.0" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["sent"], 1);
}

// ============================================================
// trigger_update: dev_only=true
// ============================================================

#[tokio::test]
async fn test_trigger_update_dev_only() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/trigger-update"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({
            "version_code": 99,
            "dev_only": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// trigger_update_dev: with tenants
// ============================================================

#[tokio::test]
async fn test_trigger_update_dev_with_tenants() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("FCM_INTERNAL_SECRET", "dev-secret-2");

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_dev_tenants.store(true, Ordering::SeqCst);
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/trigger-update-dev"))
        .header("X-Internal-Secret", "dev-secret-2")
        .json(&serde_json::json!({ "version_code": 100 }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    // Devices have version_code=10 < 100, so should send
    assert_eq!(body["sent"], 1);
    std::env::remove_var("FCM_INTERNAL_SECRET");
}

// ============================================================
// trigger_update_dev: DB error on list_dev_device_tenant_ids
// ============================================================

#[tokio::test]
async fn test_trigger_update_dev_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("FCM_INTERNAL_SECRET", "dev-secret-3");

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/trigger-update-dev"))
        .header("X-Internal-Secret", "dev-secret-3")
        .json(&serde_json::json!({ "version_code": 100 }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
    std::env::remove_var("FCM_INTERNAL_SECRET");
}

// ============================================================
// trigger_update_dev: no FCM configured
// ============================================================

#[tokio::test]
async fn test_trigger_update_dev_no_fcm() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("FCM_INTERNAL_SECRET", "dev-secret-4");

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_dev_tenants.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    // fcm = None -> send_update_fcm returns 503 but trigger_update_dev catches it with if let Ok
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/trigger-update-dev"))
        .header("X-Internal-Secret", "dev-secret-4")
        .json(&serde_json::json!({ "version_code": 100 }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["sent"], 0);
    std::env::remove_var("FCM_INTERNAL_SECRET");
}

// ============================================================
// claim_registration: qr_permanent with no device_name
// ============================================================

#[tokio::test]
async fn test_claim_qr_permanent_no_device_name() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    mock.return_qr_permanent.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({
            "registration_code": "QRCODE2"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], true);
    assert_eq!(body["flow_type"], "qr_permanent");
}

// ============================================================
// claim_registration: device_owner flow
// ============================================================

#[tokio::test]
async fn test_claim_device_owner_flow() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    // Default flow_type is "url", which covers url|device_owner match arm
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({
            "registration_code": "DOCODE",
            "device_name": ""
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], true);
    assert_eq!(body["flow_type"], "url");
}

// ============================================================
// fcm_notify_call: correct secret header
// ============================================================

#[tokio::test]
async fn test_fcm_notify_call_correct_secret() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("FCM_INTERNAL_SECRET", "correct-secret-2");

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(crate::common::MockFcmSender::new()));
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/devices/fcm-notify-call"))
        .header("X-Internal-Secret", "correct-secret-2")
        .json(&serde_json::json!({ "room_ids": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    std::env::remove_var("FCM_INTERNAL_SECRET");
}

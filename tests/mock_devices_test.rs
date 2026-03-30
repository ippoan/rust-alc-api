#[macro_use]
mod common;
mod mock_helpers;

use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde_json::Value;

use mock_helpers::app_state::setup_mock_app_state;
use mock_helpers::MockDeviceRepository;

// ============================================================
// POST /devices/register/request — public (no auth)
// ============================================================

#[tokio::test]
async fn test_register_request_success() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    // return_data=false -> None -> 404
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    // return_data=false -> None -> error
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst); // lookup_device_tenant returns Some
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    // return_data=false -> lookup_device_tenant returns None -> 404
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);
    std::env::remove_var("FCM_INTERNAL_SECRET");

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    // fcm = None
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);
    std::env::remove_var("FCM_INTERNAL_SECRET");

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(common::MockFcmSender::new()));
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);
    std::env::remove_var("FCM_INTERNAL_SECRET");

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(common::MockFcmSender::new()));
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);
    std::env::set_var("FCM_INTERNAL_SECRET", "my-secret-123");

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(common::MockFcmSender::new()));
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    // fcm = None
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    // return_data=false -> get_device_tenant_active returns None -> 404
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(common::MockFcmSender::new()));
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(common::MockFcmSender::new()));
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    // return_data=false -> find_approve_request returns None -> 404
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(common::MockFcmSender::new()));
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(common::MockFcmSender::new()));
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    // fcm = None
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    // return_data=false -> get_device_fcm_token returns None -> 404
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(common::MockFcmSender::new()));
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(common::FailingFcmSender));
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(common::MockFcmSender::new()));
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(common::MockFcmSender::new()));
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(common::MockFcmSender::new()));
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);
    std::env::remove_var("FCM_INTERNAL_SECRET");

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(common::MockFcmSender::new()));
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);
    std::env::set_var("FCM_INTERNAL_SECRET", "correct-secret");

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(common::MockFcmSender::new()));
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);
    std::env::set_var("FCM_INTERNAL_SECRET", "dev-secret");

    let mock = Arc::new(MockDeviceRepository::default());
    // return_data=false -> list_dev_device_tenant_ids returns empty vec -> no devices
    let mut state = setup_mock_app_state();
    state.devices = mock;
    state.fcm = Some(Arc::new(common::MockFcmSender::new()));
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDeviceRepository::default());
    let mut state = setup_mock_app_state();
    state.devices = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/devices"))
        .header("X-Tenant-ID", Uuid::new_v4().to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

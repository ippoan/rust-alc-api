#[macro_use]
mod common;
mod mock_helpers;

use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde_json::Value;

use mock_helpers::app_state::setup_mock_app_state;
use mock_helpers::MockTenkoCallRepository;

// ============================================================
// POST /tenko-call/register — public (no auth)
// ============================================================

#[tokio::test]
async fn test_register_success() {
    test_group!("TenkoCall: register success");
    test_case!("call_number がマスタに存在し登録成功", {
        let _guard = common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

        let mock = Arc::new(MockTenkoCallRepository::default());
        mock.return_some.store(true, Ordering::SeqCst);
        let mut state = setup_mock_app_state();
        state.tenko_call = mock;
        let base_url = common::spawn_test_server(state).await;

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko-call/register"))
            .json(&serde_json::json!({
                "phone_number": "080-1111-2222",
                "driver_name": "Test Driver",
                "call_number": "090-0000-0001",
                "employee_code": "EMP001"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["success"], true);
        assert_eq!(body["driver_id"], 42);
        assert_eq!(body["call_number"], "090-0000-0001");
        assert!(body.get("error").is_none() || body["error"].is_null());
    });
}

#[tokio::test]
async fn test_register_without_employee_code() {
    test_group!("TenkoCall: register without employee_code");
    test_case!("employee_code なしでも登録成功", {
        let _guard = common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

        let mock = Arc::new(MockTenkoCallRepository::default());
        mock.return_some.store(true, Ordering::SeqCst);
        let mut state = setup_mock_app_state();
        state.tenko_call = mock;
        let base_url = common::spawn_test_server(state).await;

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko-call/register"))
            .json(&serde_json::json!({
                "phone_number": "080-3333-4444",
                "driver_name": "Driver No Code",
                "call_number": "090-0000-0001"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["success"], true);
        assert_eq!(body["driver_id"], 42);
    });
}

#[tokio::test]
async fn test_register_unknown_call_number() {
    test_group!("TenkoCall: register unknown call_number");
    test_case!("未登録の call_number は 400", {
        let _guard = common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

        let mock = Arc::new(MockTenkoCallRepository::default());
        // return_some = false (default) → Ok(None) → BAD_REQUEST
        let mut state = setup_mock_app_state();
        state.tenko_call = mock;
        let base_url = common::spawn_test_server(state).await;

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko-call/register"))
            .json(&serde_json::json!({
                "phone_number": "080-1111-2222",
                "driver_name": "Test Driver",
                "call_number": "999-9999-9999"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["success"], false);
        assert_eq!(body["driver_id"], 0);
        assert!(body["error"].as_str().unwrap().contains("未登録"));
    });
}

#[tokio::test]
async fn test_register_db_error() {
    test_group!("TenkoCall: register DB error");
    test_case!("DB エラー時に 500 を返す", {
        let _guard = common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

        let mock = Arc::new(MockTenkoCallRepository::default());
        mock.fail_next.store(true, Ordering::SeqCst);
        let mut state = setup_mock_app_state();
        state.tenko_call = mock;
        let base_url = common::spawn_test_server(state).await;

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko-call/register"))
            .json(&serde_json::json!({
                "phone_number": "080-1111-2222",
                "driver_name": "Test Driver",
                "call_number": "090-0000-0001"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(body["error"].as_str().unwrap().contains("internal error"));
    });
}

// ============================================================
// POST /tenko-call/tenko — public (no auth)
// ============================================================

#[tokio::test]
async fn test_tenko_success() {
    test_group!("TenkoCall: tenko success");
    test_case!("登録済みドライバーの点呼が成功", {
        let _guard = common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

        let mock = Arc::new(MockTenkoCallRepository::default());
        mock.return_some.store(true, Ordering::SeqCst);
        let mut state = setup_mock_app_state();
        state.tenko_call = mock;
        let base_url = common::spawn_test_server(state).await;

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko-call/tenko"))
            .json(&serde_json::json!({
                "phone_number": "080-1111-2222",
                "driver_name": "Test Driver",
                "latitude": 35.6812,
                "longitude": 139.7671
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["success"], true);
        assert_eq!(body["call_number"], "090-1234-5678");
    });
}

#[tokio::test]
async fn test_tenko_driver_not_found() {
    test_group!("TenkoCall: tenko driver not found");
    test_case!("未登録ドライバーの点呼は 404", {
        let _guard = common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

        let mock = Arc::new(MockTenkoCallRepository::default());
        // return_some = false (default) → Ok(None) → NOT_FOUND
        let mut state = setup_mock_app_state();
        state.tenko_call = mock;
        let base_url = common::spawn_test_server(state).await;

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko-call/tenko"))
            .json(&serde_json::json!({
                "phone_number": "080-9999-9999",
                "driver_name": "Unknown",
                "latitude": 35.0,
                "longitude": 139.0
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

#[tokio::test]
async fn test_tenko_db_error() {
    test_group!("TenkoCall: tenko DB error");
    test_case!("DB エラー時に 500 を返す", {
        let _guard = common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

        let mock = Arc::new(MockTenkoCallRepository::default());
        mock.fail_next.store(true, Ordering::SeqCst);
        let mut state = setup_mock_app_state();
        state.tenko_call = mock;
        let base_url = common::spawn_test_server(state).await;

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko-call/tenko"))
            .json(&serde_json::json!({
                "phone_number": "080-1111-2222",
                "driver_name": "Test Driver",
                "latitude": 35.0,
                "longitude": 139.0
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

// ============================================================
// GET /tenko-call/numbers — tenant auth required
// ============================================================

#[tokio::test]
async fn test_list_numbers_empty() {
    test_group!("TenkoCall: list_numbers empty");
    test_case!("空のリストを取得できる", {
        let _guard = common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

        let mock = Arc::new(MockTenkoCallRepository::default());
        let mut state = setup_mock_app_state();
        state.tenko_call = mock;
        let base_url = common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .get(format!("{base_url}/api/tenko-call/numbers"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body.as_array().unwrap().len(), 0);
    });
}

#[tokio::test]
async fn test_list_numbers_with_data() {
    test_group!("TenkoCall: list_numbers with data");
    test_case!("データがある場合にリストを取得できる", {
        let _guard = common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

        let mock = Arc::new(MockTenkoCallRepository::default());
        mock.return_data.store(true, Ordering::SeqCst);
        let mut state = setup_mock_app_state();
        state.tenko_call = mock;
        let base_url = common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .get(format!("{base_url}/api/tenko-call/numbers"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        let arr = body.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], 1);
        assert_eq!(arr[0]["call_number"], "090-0000-0001");
        assert_eq!(arr[0]["tenant_id"], "test-tenant");
        assert_eq!(arr[0]["label"], "Office A");
        assert_eq!(arr[0]["created_at"], "2026-01-01 00:00:00");
    });
}

#[tokio::test]
async fn test_list_numbers_with_x_tenant_id() {
    test_group!("TenkoCall: list_numbers with X-Tenant-ID");
    test_case!("X-Tenant-ID ヘッダーでもアクセスできる", {
        let _guard = common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

        let mock = Arc::new(MockTenkoCallRepository::default());
        let mut state = setup_mock_app_state();
        state.tenko_call = mock;
        let base_url = common::spawn_test_server(state).await;

        let client = reqwest::Client::new();
        let res = client
            .get(format!("{base_url}/api/tenko-call/numbers"))
            .header("X-Tenant-ID", Uuid::new_v4().to_string())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    });
}

#[tokio::test]
async fn test_list_numbers_no_auth() {
    test_group!("TenkoCall: list_numbers no auth");
    test_case!("認証なしは 401", {
        let _guard = common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

        let mock = Arc::new(MockTenkoCallRepository::default());
        let mut state = setup_mock_app_state();
        state.tenko_call = mock;
        let base_url = common::spawn_test_server(state).await;

        let client = reqwest::Client::new();
        let res = client
            .get(format!("{base_url}/api/tenko-call/numbers"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 401);
    });
}

#[tokio::test]
async fn test_list_numbers_db_error() {
    test_group!("TenkoCall: list_numbers DB error");
    test_case!("DB エラー時に 500 を返す", {
        let _guard = common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

        let mock = Arc::new(MockTenkoCallRepository::default());
        mock.fail_next.store(true, Ordering::SeqCst);
        let mut state = setup_mock_app_state();
        state.tenko_call = mock;
        let base_url = common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .get(format!("{base_url}/api/tenko-call/numbers"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

// ============================================================
// POST /tenko-call/numbers — tenant auth required
// ============================================================

#[tokio::test]
async fn test_create_number_success() {
    test_group!("TenkoCall: create_number success");
    test_case!("電話番号マスタの追加が成功する", {
        let _guard = common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

        let mock = Arc::new(MockTenkoCallRepository::default());
        let mut state = setup_mock_app_state();
        state.tenko_call = mock;
        let base_url = common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/tenko-call/numbers"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({
                "call_number": "090-0000-0099",
                "tenant_id": tenant_id.to_string(),
                "label": "New Office"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["success"], true);
        assert_eq!(body["id"], 99);
    });
}

#[tokio::test]
async fn test_create_number_without_tenant_id() {
    test_group!("TenkoCall: create_number without tenant_id");
    test_case!("tenant_id 省略時はデフォルト値を使用", {
        let _guard = common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

        let mock = Arc::new(MockTenkoCallRepository::default());
        let mut state = setup_mock_app_state();
        state.tenko_call = mock;
        let base_url = common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/tenko-call/numbers"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({
                "call_number": "090-0000-0100"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["success"], true);
        assert_eq!(body["id"], 99);
    });
}

#[tokio::test]
async fn test_create_number_without_label() {
    test_group!("TenkoCall: create_number without label");
    test_case!("label 省略でも成功", {
        let _guard = common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

        let mock = Arc::new(MockTenkoCallRepository::default());
        let mut state = setup_mock_app_state();
        state.tenko_call = mock;
        let base_url = common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/tenko-call/numbers"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({
                "call_number": "090-0000-0200",
                "tenant_id": "custom-tenant"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["success"], true);
    });
}

#[tokio::test]
async fn test_create_number_no_auth() {
    test_group!("TenkoCall: create_number no auth");
    test_case!("認証なしは 401", {
        let _guard = common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

        let mock = Arc::new(MockTenkoCallRepository::default());
        let mut state = setup_mock_app_state();
        state.tenko_call = mock;
        let base_url = common::spawn_test_server(state).await;

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko-call/numbers"))
            .json(&serde_json::json!({
                "call_number": "090-0000-0099"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 401);
    });
}

#[tokio::test]
async fn test_create_number_db_error() {
    test_group!("TenkoCall: create_number DB error");
    test_case!("DB エラー時に 500 を返す", {
        let _guard = common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

        let mock = Arc::new(MockTenkoCallRepository::default());
        mock.fail_next.store(true, Ordering::SeqCst);
        let mut state = setup_mock_app_state();
        state.tenko_call = mock;
        let base_url = common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/tenko-call/numbers"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({
                "call_number": "090-0000-0099"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

// ============================================================
// DELETE /tenko-call/numbers/{id} — tenant auth required
// ============================================================

#[tokio::test]
async fn test_delete_number_success() {
    test_group!("TenkoCall: delete_number success");
    test_case!("電話番号マスタの削除が成功する", {
        let _guard = common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

        let mock = Arc::new(MockTenkoCallRepository::default());
        let mut state = setup_mock_app_state();
        state.tenko_call = mock;
        let base_url = common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .delete(format!("{base_url}/api/tenko-call/numbers/1"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 204);
    });
}

#[tokio::test]
async fn test_delete_number_no_auth() {
    test_group!("TenkoCall: delete_number no auth");
    test_case!("認証なしは 401", {
        let _guard = common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

        let mock = Arc::new(MockTenkoCallRepository::default());
        let mut state = setup_mock_app_state();
        state.tenko_call = mock;
        let base_url = common::spawn_test_server(state).await;

        let client = reqwest::Client::new();
        let res = client
            .delete(format!("{base_url}/api/tenko-call/numbers/1"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 401);
    });
}

#[tokio::test]
async fn test_delete_number_db_error() {
    test_group!("TenkoCall: delete_number DB error");
    test_case!("DB エラー時に 500 を返す", {
        let _guard = common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

        let mock = Arc::new(MockTenkoCallRepository::default());
        mock.fail_next.store(true, Ordering::SeqCst);
        let mut state = setup_mock_app_state();
        state.tenko_call = mock;
        let base_url = common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .delete(format!("{base_url}/api/tenko-call/numbers/1"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

// ============================================================
// GET /tenko-call/drivers — tenant auth required
// ============================================================

#[tokio::test]
async fn test_list_drivers_empty() {
    test_group!("TenkoCall: list_drivers empty");
    test_case!("空のリストを取得できる", {
        let _guard = common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

        let mock = Arc::new(MockTenkoCallRepository::default());
        let mut state = setup_mock_app_state();
        state.tenko_call = mock;
        let base_url = common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .get(format!("{base_url}/api/tenko-call/drivers"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body.as_array().unwrap().len(), 0);
    });
}

#[tokio::test]
async fn test_list_drivers_with_data() {
    test_group!("TenkoCall: list_drivers with data");
    test_case!("データがある場合にリストを取得できる", {
        let _guard = common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

        let mock = Arc::new(MockTenkoCallRepository::default());
        mock.return_data.store(true, Ordering::SeqCst);
        let mut state = setup_mock_app_state();
        state.tenko_call = mock;
        let base_url = common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .get(format!("{base_url}/api/tenko-call/drivers"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        let arr = body.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], 1);
        assert_eq!(arr[0]["phone_number"], "080-1111-2222");
        assert_eq!(arr[0]["driver_name"], "Test Driver");
        assert_eq!(arr[0]["call_number"], "090-0000-0001");
        assert_eq!(arr[0]["tenant_id"], "test-tenant");
        assert_eq!(arr[0]["employee_code"], "EMP001");
        assert_eq!(arr[0]["created_at"], "2026-01-01 00:00:00");
    });
}

#[tokio::test]
async fn test_list_drivers_no_auth() {
    test_group!("TenkoCall: list_drivers no auth");
    test_case!("認証なしは 401", {
        let _guard = common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

        let mock = Arc::new(MockTenkoCallRepository::default());
        let mut state = setup_mock_app_state();
        state.tenko_call = mock;
        let base_url = common::spawn_test_server(state).await;

        let client = reqwest::Client::new();
        let res = client
            .get(format!("{base_url}/api/tenko-call/drivers"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 401);
    });
}

#[tokio::test]
async fn test_list_drivers_db_error() {
    test_group!("TenkoCall: list_drivers DB error");
    test_case!("DB エラー時に 500 を返す", {
        let _guard = common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

        let mock = Arc::new(MockTenkoCallRepository::default());
        mock.fail_next.store(true, Ordering::SeqCst);
        let mut state = setup_mock_app_state();
        state.tenko_call = mock;
        let base_url = common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .get(format!("{base_url}/api/tenko-call/drivers"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

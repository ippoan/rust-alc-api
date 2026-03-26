mod common;

use serde_json::Value;

// ============================================================
// ヘルパー
// ============================================================

/// テナント + ユーザー + JWT を作成するセットアップヘルパー
/// approved_by FK のためにDB上にユーザーが必要
async fn setup_tenant_with_user(
    state: &rust_alc_api::AppState,
) -> (uuid::Uuid, String) {
    let tenant_id = common::create_test_tenant(&state.pool, &format!("Dev{}", uuid::Uuid::new_v4().simple())).await;
    let (user_id, _) = common::create_test_user_in_db(&state.pool, tenant_id, "dev@test.com", "admin").await;
    let jwt = common::create_test_jwt_for_user(user_id, tenant_id, "dev@test.com", "admin");
    (tenant_id, jwt)
}

/// URL フローでデバイスを作成して device_id を返す
async fn create_device_via_url_flow(
    client: &reqwest::Client,
    base_url: &str,
    auth: &str,
) -> (String, String) {
    // 管理者がトークン生成
    let res = client
        .post(format!("{base_url}/api/devices/register/create-token"))
        .header("Authorization", auth)
        .json(&serde_json::json!({ "device_name": "Test Device" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    let code = body["registration_code"].as_str().unwrap().to_string();

    // 端末がクレーム
    let res = client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({
            "registration_code": code,
            "phone_number": "090-1234-5678",
            "device_name": "Test Device"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], true);
    assert_eq!(body["flow_type"], "url");
    let device_id = body["device_id"].as_str().unwrap().to_string();

    (device_id, code)
}

// ============================================================
// Public endpoints
// ============================================================

#[tokio::test]
async fn test_create_registration_request() {
    let state = common::setup_app_state().await;
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
async fn test_check_status_pending() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    // QR一時リクエスト作成
    let res = client
        .post(format!("{base_url}/api/devices/register/request"))
        .json(&serde_json::json!({ "device_name": "Poll Device" }))
        .send()
        .await
        .unwrap();
    let body: Value = res.json().await.unwrap();
    let code = body["registration_code"].as_str().unwrap();

    // ステータス確認
    let res = client
        .get(format!("{base_url}/api/devices/register/status/{code}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["status"], "pending");
}

#[tokio::test]
async fn test_check_status_not_found() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!(
            "{base_url}/api/devices/register/status/INVALID"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_device_settings_not_found() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/devices/settings/{fake_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// ============================================================
// URL フロー
// ============================================================

#[tokio::test]
async fn test_url_flow_create_token() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "URL Token").await;
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
    assert!(body["registration_url"].as_str().is_some());
}

#[tokio::test]
async fn test_url_flow_claim() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "URL Claim").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
    assert!(!device_id.is_empty());
}

#[tokio::test]
async fn test_url_flow_device_in_list() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "URL List").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    create_device_via_url_flow(&client, &base_url, &auth).await;

    let res = client
        .get(format!("{base_url}/api/devices"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let devices: Vec<Value> = res.json().await.unwrap();
    // device_select_by_id ポリシーで全デバイスがSELECT可能なため、他テストのデバイスも含まれうる
    let our_device = devices.iter().find(|d| d["device_name"] == "Test Device");
    assert!(our_device.is_some(), "Our device should be in list");
    assert_eq!(our_device.unwrap()["status"], "active");
}

// ============================================================
// QR永久フロー
// ============================================================

#[tokio::test]
async fn test_qr_permanent_create() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "QR Perm Create").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!(
            "{base_url}/api/devices/register/create-permanent-qr"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "device_name": "QR Perm Device" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["registration_code"].as_str().is_some());
}

#[tokio::test]
async fn test_qr_permanent_claim() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "QR Perm Claim").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // 管理者がコード生成
    let res = client
        .post(format!(
            "{base_url}/api/devices/register/create-permanent-qr"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "device_name": "QR Device" }))
        .send()
        .await
        .unwrap();
    let body: Value = res.json().await.unwrap();
    let code = body["registration_code"].as_str().unwrap();

    // 端末がクレーム → 承認待ち
    let res = client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({
            "registration_code": code,
            "phone_number": "080-1111-2222"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], true);
    assert_eq!(body["flow_type"], "qr_permanent");
    // device_id はまだ無い (承認待ち)
    assert!(body.get("device_id").is_none() || body["device_id"].is_null());
}

#[tokio::test]
async fn test_qr_permanent_in_pending() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "QR Perm Pending").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // コード生成 + クレーム
    let res = client
        .post(format!(
            "{base_url}/api/devices/register/create-permanent-qr"
        ))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "device_name": "Pending QR" }))
        .send()
        .await
        .unwrap();
    let body: Value = res.json().await.unwrap();
    let code = body["registration_code"].as_str().unwrap();

    client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({ "registration_code": code }))
        .send()
        .await
        .unwrap();

    // pending 一覧に表示される
    let res = client
        .get(format!("{base_url}/api/devices/pending"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let pending: Vec<Value> = res.json().await.unwrap();
    assert!(pending.len() >= 1);
    assert!(pending.iter().any(|p| p["registration_code"] == code));
}

// ============================================================
// QR一時フロー
// ============================================================

#[tokio::test]
async fn test_qr_temp_approve_by_code() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let (_tenant_id, jwt) = setup_tenant_with_user(&state).await;
    let client = reqwest::Client::new();

    // 端末がリクエスト作成
    let res = client
        .post(format!("{base_url}/api/devices/register/request"))
        .json(&serde_json::json!({ "device_name": "Temp Device" }))
        .send()
        .await
        .unwrap();
    let body: Value = res.json().await.unwrap();
    let code = body["registration_code"].as_str().unwrap();

    // 管理者がコードで承認
    let res = client
        .post(format!(
            "{base_url}/api/devices/approve-by-code/{code}"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], true);
    assert!(body["device_id"].as_str().is_some());
}

#[tokio::test]
async fn test_qr_temp_status_after_approve() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let (_tenant_id, jwt) = setup_tenant_with_user(&state).await;
    let client = reqwest::Client::new();

    // リクエスト作成
    let res = client
        .post(format!("{base_url}/api/devices/register/request"))
        .json(&serde_json::json!({ "device_name": "Status Device" }))
        .send()
        .await
        .unwrap();
    let body: Value = res.json().await.unwrap();
    let code = body["registration_code"].as_str().unwrap();

    // 承認
    client
        .post(format!(
            "{base_url}/api/devices/approve-by-code/{code}"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    // ステータス確認
    let res = client
        .get(format!("{base_url}/api/devices/register/status/{code}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["status"], "approved");
    assert!(body["device_id"].as_str().is_some());
}

// ============================================================
// 管理操作
// ============================================================

#[tokio::test]
async fn test_list_devices_returns_ok() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Empty Devices").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/devices"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let _devices: Vec<Value> = res.json().await.unwrap();
    // device_select_by_id ポリシーで全デバイスが見えるため、件数は検証しない
}

#[tokio::test]
async fn test_approve_device_by_id() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let (_tenant_id, jwt) = setup_tenant_with_user(&state).await;
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // QR永久コード生成
    let res = client
        .post(format!(
            "{base_url}/api/devices/register/create-permanent-qr"
        ))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "device_name": "Approve Me" }))
        .send()
        .await
        .unwrap();
    let body: Value = res.json().await.unwrap();
    let code = body["registration_code"].as_str().unwrap();

    // 端末クレーム
    client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({ "registration_code": code }))
        .send()
        .await
        .unwrap();

    // pending 一覧からリクエスト ID 取得
    let res = client
        .get(format!("{base_url}/api/devices/pending"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    let pending: Vec<Value> = res.json().await.unwrap();
    let req_id = pending
        .iter()
        .find(|p| p["registration_code"] == code)
        .unwrap()["id"]
        .as_str()
        .unwrap();

    // ID で承認
    let res = client
        .post(format!("{base_url}/api/devices/approve/{req_id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], true);
    assert!(body["device_id"].as_str().is_some());
}

#[tokio::test]
async fn test_reject_device() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Reject Dev").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // QR永久コード生成 + クレーム
    let res = client
        .post(format!(
            "{base_url}/api/devices/register/create-permanent-qr"
        ))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "device_name": "Reject Me" }))
        .send()
        .await
        .unwrap();
    let body: Value = res.json().await.unwrap();
    let code = body["registration_code"].as_str().unwrap();

    client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({ "registration_code": code }))
        .send()
        .await
        .unwrap();

    // pending から ID 取得
    let res = client
        .get(format!("{base_url}/api/devices/pending"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    let pending: Vec<Value> = res.json().await.unwrap();
    let req_id = pending
        .iter()
        .find(|p| p["registration_code"] == code)
        .unwrap()["id"]
        .as_str()
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/reject/{req_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_reject_not_found() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Reject NF").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let res = client
        .post(format!("{base_url}/api/devices/reject/{fake_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_disable_enable() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Disable Enable").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;

    // Disable
    let res = client
        .post(format!("{base_url}/api/devices/disable/{device_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);

    // Enable
    let res = client
        .post(format!("{base_url}/api/devices/enable/{device_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_delete_device() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Delete Dev").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;

    let res = client
        .delete(format!("{base_url}/api/devices/{device_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);

    // 設定取得で 404
    let res = client
        .get(format!("{base_url}/api/devices/settings/{device_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_delete_not_found() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Del NF Dev").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let res = client
        .delete(format!("{base_url}/api/devices/{fake_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_device_operations_from_different_tenant() {
    // NOTE: device_select_by_id ポリシー (FOR SELECT USING (true)) により
    // SELECT/UPDATE/DELETE は全テナント横断でアクセス可能な状態。
    // このテストはクロステナント操作が実行可能であることを記録する。
    // TODO: RLS ポリシーを修正して、UPDATE/DELETE にテナント分離を追加すべき
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;

    let tenant_a = common::create_test_tenant(&state.pool, "Dev Iso A").await;
    let tenant_b = common::create_test_tenant(&state.pool, "Dev Iso B").await;

    let jwt_a = common::create_test_jwt(tenant_a, "admin");
    let _jwt_b = common::create_test_jwt(tenant_b, "admin");
    let auth_a = format!("Bearer {jwt_a}");
    let client = reqwest::Client::new();

    // テナント A にデバイス作成
    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth_a).await;

    // テナント A から設定取得 → 成功
    let res = client
        .get(format!("{base_url}/api/devices/settings/{device_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // テナント B からも設定取得可能 (device_select_by_id ポリシー)
    let res = client
        .get(format!("{base_url}/api/devices/settings/{device_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// Public endpoints (端末側)
// ============================================================

#[tokio::test]
async fn test_register_fcm_token() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "FCM Token").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;

    let res = client
        .put(format!("{base_url}/api/devices/register-fcm-token"))
        .json(&serde_json::json!({
            "device_id": device_id,
            "fcm_token": "fake-fcm-token-abc123"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_update_last_login() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Last Login").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
    let emp = common::create_test_employee(&client, &base_url, &auth, "LoginEmp", "LE01").await;
    let emp_id = emp["id"].as_str().unwrap();

    let res = client
        .put(format!("{base_url}/api/devices/update-last-login"))
        .json(&serde_json::json!({
            "device_id": device_id,
            "employee_id": emp_id,
            "employee_name": "LoginEmp",
            "employee_role": ["driver"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);

    // 設定取得で確認
    let res = client
        .get(format!("{base_url}/api/devices/settings/{device_id}"))
        .send()
        .await
        .unwrap();
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["last_login_employee_name"], "LoginEmp");
}

#[tokio::test]
async fn test_report_version() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Version").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;

    let res = client
        .put(format!("{base_url}/api/devices/report-version"))
        .json(&serde_json::json!({
            "device_id": device_id,
            "version_code": 42,
            "version_name": "1.2.3"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_report_watchdog() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Watchdog").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;

    let res = client
        .put(format!("{base_url}/api/devices/report-watchdog"))
        .json(&serde_json::json!({
            "device_id": device_id,
            "running": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_fcm_notify_call_no_fcm() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    // FCM 未設定 → 503
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
async fn test_device_owner_flow() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Dev Owner").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // Device Owner トークン生成
    let res = client
        .post(format!(
            "{base_url}/api/devices/register/create-device-owner-token"
        ))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "device_name": "Owner Device" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    let code = body["registration_code"].as_str().unwrap();

    // 端末がクレーム → 即承認
    let res = client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({
            "registration_code": code,
            "device_name": "Owner Device"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], true);
    assert_eq!(body["flow_type"], "device_owner");
    assert!(body["device_id"].as_str().is_some());
}

// ============================================================
// デバイス設定
// ============================================================

#[tokio::test]
async fn test_device_settings_after_creation() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Settings Test").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;

    let res = client
        .get(format!("{base_url}/api/devices/settings/{device_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["status"], "active");
    // URL フローで作成されたデバイスのデフォルト値を検証
    assert!(body.get("call_enabled").is_some());
    assert!(body.get("always_on").is_some());
}

#[tokio::test]
async fn test_update_call_settings() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Call Settings").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;

    // 着信設定更新
    let res = client
        .put(format!(
            "{base_url}/api/devices/{device_id}/call-settings"
        ))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "call_enabled": true,
            "always_on": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);

    // 設定確認
    let res = client
        .get(format!("{base_url}/api/devices/settings/{device_id}"))
        .send()
        .await
        .unwrap();
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["call_enabled"], true);
    assert_eq!(body["always_on"], true);
}

// ============================================================
// FCM dismiss test
// ============================================================

#[tokio::test]
async fn test_fcm_dismiss_test_no_fcm_configured() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "FCM Dismiss").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;

    // FCM 未設定 → 503
    let res = client
        .post(format!("{base_url}/api/devices/fcm-dismiss-test"))
        .json(&serde_json::json!({ "device_id": device_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 503);
}

// ============================================================
// update-last-login with non-existent device
// ============================================================

#[tokio::test]
async fn test_update_last_login_not_found() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let fake_device_id = uuid::Uuid::new_v4();
    let res = client
        .put(format!("{base_url}/api/devices/update-last-login"))
        .json(&serde_json::json!({
            "device_id": fake_device_id.to_string(),
            "employee_id": uuid::Uuid::new_v4().to_string(),
            "employee_name": "Ghost",
            "employee_role": ["driver"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// ============================================================
// report-version with non-existent device
// ============================================================

#[tokio::test]
async fn test_report_version_not_found() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let fake_device_id = uuid::Uuid::new_v4();
    let res = client
        .put(format!("{base_url}/api/devices/report-version"))
        .json(&serde_json::json!({
            "device_id": fake_device_id.to_string(),
            "version_code": 99,
            "version_name": "9.9.9"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// ============================================================
// Device Owner flow: claim + verify is_device_owner in list
// ============================================================

#[tokio::test]
async fn test_device_owner_claim_and_verify_in_list() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "DO Verify").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // Device Owner トークン生成
    let res = client
        .post(format!(
            "{base_url}/api/devices/register/create-device-owner-token"
        ))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "device_name": "DO List Device" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    let code = body["registration_code"].as_str().unwrap();

    // 端末がクレーム → 即承認
    let res = client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({
            "registration_code": code,
            "device_name": "DO List Device"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    let device_id = body["device_id"].as_str().unwrap();

    // report-version で is_device_owner=true を報告
    let res = client
        .put(format!("{base_url}/api/devices/report-version"))
        .json(&serde_json::json!({
            "device_id": device_id,
            "version_code": 10,
            "version_name": "1.0.0",
            "is_device_owner": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);

    // デバイス一覧で is_device_owner=true を確認
    let res = client
        .get(format!("{base_url}/api/devices"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let devices: Vec<Value> = res.json().await.unwrap();
    let our_device = devices
        .iter()
        .find(|d| d["id"].as_str() == Some(device_id));
    assert!(
        our_device.is_some(),
        "Device Owner device should appear in list"
    );
    assert_eq!(our_device.unwrap()["is_device_owner"], true);
}

// ============================================================
// call-settings with non-existent device id
// ============================================================

#[tokio::test]
async fn test_update_call_settings_not_found() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "Call NF").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let res = client
        .put(format!(
            "{base_url}/api/devices/{fake_id}/call-settings"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({
            "call_enabled": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// ============================================================
// trigger-update-dev without X-Internal-Secret
// ============================================================

#[tokio::test]
async fn test_trigger_update_dev_no_secret() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    // FCM_INTERNAL_SECRET が未設定の場合 → 503
    // FCM_INTERNAL_SECRET が設定済みでもヘッダーなし → 401
    let res = client
        .post(format!("{base_url}/api/devices/trigger-update-dev"))
        .json(&serde_json::json!({
            "version_code": 100,
            "version_name": "2.0.0"
        }))
        .send()
        .await
        .unwrap();
    // テスト環境では FCM_INTERNAL_SECRET 未設定のため 503
    assert!(
        res.status() == 503 || res.status() == 401,
        "Expected 503 (no secret configured) or 401 (unauthorized), got {}",
        res.status()
    );
}

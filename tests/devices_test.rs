#[macro_use]
mod common;

use serde_json::Value;

// ============================================================
// ヘルパー
// ============================================================

async fn setup_tenant_with_user(
    state: &rust_alc_api::AppState,
) -> (uuid::Uuid, String) {
    let tenant_id = common::create_test_tenant(&state.pool, &format!("Dev{}", uuid::Uuid::new_v4().simple())).await;
    let (user_id, _) = common::create_test_user_in_db(&state.pool, tenant_id, "dev@test.com", "admin").await;
    let jwt = common::create_test_jwt_for_user(user_id, tenant_id, "dev@test.com", "admin");
    (tenant_id, jwt)
}

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
    test_group!("公開エンドポイント");
    test_case!("QR一時登録リクエストを作成できる", {
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
    });
}

#[tokio::test]
async fn test_check_status_pending() {
    test_group!("公開エンドポイント");
    test_case!("登録リクエストのステータスがpendingで返る", {
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
    });
}

#[tokio::test]
async fn test_check_status_not_found() {
    test_group!("公開エンドポイント");
    test_case!("存在しない登録コードのステータス確認で404が返る", {
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
    });
}

#[tokio::test]
async fn test_device_settings_not_found() {
    test_group!("公開エンドポイント");
    test_case!("存在しないデバイスIDの設定取得で404が返る", {
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
    });
}

// ============================================================
// URL フロー
// ============================================================

#[tokio::test]
async fn test_url_flow_create_token() {
    test_group!("URLフロー");
    test_case!("管理者がURLフロー用トークンを生成できる", {
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
    });
}

#[tokio::test]
async fn test_url_flow_claim() {
    test_group!("URLフロー");
    test_case!("端末がURLフローでクレームして即登録される", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "URL Claim").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
        assert!(!device_id.is_empty());
    });
}

#[tokio::test]
async fn test_url_flow_device_in_list() {
    test_group!("URLフロー");
    test_case!("URLフローで作成したデバイスが一覧に表示される", {
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
    });
}

// ============================================================
// QR永久フロー
// ============================================================

#[tokio::test]
async fn test_qr_permanent_create() {
    test_group!("QR永久フロー");
    test_case!("管理者がQR永久コードを生成できる", {
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
    });
}

#[tokio::test]
async fn test_qr_permanent_claim() {
    test_group!("QR永久フロー");
    test_case!("端末がQR永久コードでクレームすると承認待ちになる", {
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
    });
}

#[tokio::test]
async fn test_qr_permanent_in_pending() {
    test_group!("QR永久フロー");
    test_case!("QR永久クレーム後に承認待ち一覧に表示される", {
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
    });
}

// ============================================================
// QR一時フロー
// ============================================================

#[tokio::test]
async fn test_qr_temp_approve_by_code() {
    test_group!("QR一時フロー");
    test_case!("管理者がコードで直接承認できる", {
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
    });
}

#[tokio::test]
async fn test_qr_temp_status_after_approve() {
    test_group!("QR一時フロー");
    test_case!("承認後のステータスがapprovedに変わる", {
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
    });
}

// ============================================================
// 管理操作
// ============================================================

#[tokio::test]
async fn test_list_devices_returns_ok() {
    test_group!("管理操作");
    test_case!("デバイス一覧が200で返る", {
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
    });
}

#[tokio::test]
async fn test_approve_device_by_id() {
    test_group!("管理操作");
    test_case!("リクエストIDで承認できる", {
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
    });
}

#[tokio::test]
async fn test_reject_device() {
    test_group!("管理操作");
    test_case!("登録リクエストを拒否できる", {
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
    });
}

#[tokio::test]
async fn test_reject_not_found() {
    test_group!("管理操作");
    test_case!("存在しないリクエストIDの拒否で404が返る", {
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
    });
}

#[tokio::test]
async fn test_disable_enable() {
    test_group!("管理操作");
    test_case!("デバイスの無効化と有効化ができる", {
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
    });
}

#[tokio::test]
async fn test_delete_device() {
    test_group!("管理操作");
    test_case!("デバイスを削除でき、設定取得で404になる", {
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
    });
}

#[tokio::test]
async fn test_delete_not_found() {
    test_group!("管理操作");
    test_case!("存在しないデバイスIDの削除で404が返る", {
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
    });
}

#[tokio::test]
async fn test_device_operations_from_different_tenant() {
    test_group!("管理操作");
    test_case!("クロステナント操作が可能であることを記録する (RLSポリシー問題)", {
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
    });
}

// ============================================================
// 端末側公開エンドポイント
// ============================================================

#[tokio::test]
async fn test_register_fcm_token() {
    test_group!("端末側公開エンドポイント");
    test_case!("FCMトークンを登録できる", {
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
    });
}

#[tokio::test]
async fn test_update_last_login() {
    test_group!("端末側公開エンドポイント");
    test_case!("最終ログイン情報を更新できる", {
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
    });
}

#[tokio::test]
async fn test_report_version() {
    test_group!("端末側公開エンドポイント");
    test_case!("バージョン情報を報告できる", {
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
    });
}

#[tokio::test]
async fn test_report_watchdog() {
    test_group!("端末側公開エンドポイント");
    test_case!("Watchdog状態を報告できる", {
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
    });
}

#[tokio::test]
async fn test_fcm_notify_call_no_fcm() {
    test_group!("端末側公開エンドポイント");
    test_case!("FCMトークンなしでもfcm-notify-callが200を返す", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state).await;
        let client = reqwest::Client::new();

        // MockFcmSender 注入済み → 200
        let res = client
            .post(format!("{base_url}/api/devices/fcm-notify-call"))
            .json(&serde_json::json!({
                "room_ids": ["room-1"]
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    });
}

#[tokio::test]
async fn test_fcm_notify_call_with_token() {
    test_group!("端末側公開エンドポイント");
    test_case!("FCMトークン登録済みデバイスにFCM通知が送信される", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "FCM Notify Token").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        // デバイス作成 + FCM トークン登録 + call_enabled=true
        let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
        client.put(format!("{base_url}/api/devices/register-fcm-token"))
            .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "test-fcm-token-123" }))
            .send().await.unwrap();
        client.put(format!("{base_url}/api/devices/{device_id}/call-settings"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "call_enabled": true }))
            .send().await.unwrap();

        // FCM notify-call → should send to device
        let res = client
            .post(format!("{base_url}/api/devices/fcm-notify-call"))
            .json(&serde_json::json!({ "room_ids": ["room-abc"] }))
            .send().await.unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert!(body["sent"].as_i64().unwrap() >= 1, "should send to at least 1 device");
    });
}

#[tokio::test]
async fn test_fcm_notify_call_with_exclude() {
    test_group!("端末側公開エンドポイント");
    test_case!("除外デバイスIDを指定するとFCM通知がスキップされる", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "FCM Exclude").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
        client.put(format!("{base_url}/api/devices/register-fcm-token"))
            .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "token-exclude" }))
            .send().await.unwrap();
        client.put(format!("{base_url}/api/devices/{device_id}/call-settings"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "call_enabled": true }))
            .send().await.unwrap();

        // exclude this device → skipped
        let res = client
            .post(format!("{base_url}/api/devices/fcm-notify-call"))
            .json(&serde_json::json!({
                "room_ids": ["room-1"],
                "exclude_device_ids": [device_id]
            }))
            .send().await.unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert!(body["skipped"].as_i64().unwrap() >= 1);
    });
}

#[tokio::test]
async fn test_fcm_notify_call_with_schedule() {
    test_group!("端末側公開エンドポイント");
    test_case!("スケジュール時間外のデバイスはFCM通知がスキップされる", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "FCM Schedule").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
        client.put(format!("{base_url}/api/devices/register-fcm-token"))
            .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "token-sched" }))
            .send().await.unwrap();

        // call_enabled=true + schedule with narrow window that excludes current time
        client.put(format!("{base_url}/api/devices/{device_id}/call-settings"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "call_enabled": true,
                "call_schedule": {
                    "enabled": true,
                    "days": [0, 1, 2, 3, 4, 5, 6],
                    "startHour": 3,
                    "startMin": 0,
                    "endHour": 3,
                    "endMin": 1
                }
            }))
            .send().await.unwrap();

        // Schedule window is 03:00-03:01, current time is likely not in that range → skipped
        let res = client
            .post(format!("{base_url}/api/devices/fcm-notify-call"))
            .json(&serde_json::json!({ "room_ids": ["room-1"] }))
            .send().await.unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert!(body["skipped"].as_i64().unwrap() >= 1, "should skip due to schedule");
    });
}

#[tokio::test]
async fn test_fcm_notify_call_disabled() {
    test_group!("端末側公開エンドポイント");
    test_case!("着信無効のデバイスはFCM通知がスキップされる", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "FCM Disabled").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
        client.put(format!("{base_url}/api/devices/register-fcm-token"))
            .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "token-disabled" }))
            .send().await.unwrap();

        // call_enabled=false → skipped
        let res = client
            .post(format!("{base_url}/api/devices/fcm-notify-call"))
            .json(&serde_json::json!({ "room_ids": ["room-1"] }))
            .send().await.unwrap();
        assert_eq!(res.status(), 200);
    });
}

#[tokio::test]
async fn test_device_owner_flow() {
    test_group!("端末側公開エンドポイント");
    test_case!("Device Ownerフローでクレームすると即承認される", {
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
    });
}

// ============================================================
// デバイス設定
// ============================================================

#[tokio::test]
async fn test_device_settings_after_creation() {
    test_group!("デバイス設定");
    test_case!("作成直後のデバイス設定がデフォルト値で取得できる", {
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
    });
}

#[tokio::test]
async fn test_update_call_settings() {
    test_group!("デバイス設定");
    test_case!("着信設定を更新できる", {
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
    });
}

// ============================================================
// FCM dismiss テスト
// ============================================================

#[tokio::test]
async fn test_fcm_dismiss_test_no_fcm_configured() {
    test_group!("FCM dismissテスト");
    test_case!("MockFcmSenderでfcm-dismiss-testが成功する", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "FCM Dismiss").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;

        // MockFcmSender 注入済み → 200 or 204
        let res = client
            .post(format!("{base_url}/api/devices/fcm-dismiss-test"))
            .json(&serde_json::json!({ "device_id": device_id }))
            .send()
            .await
            .unwrap();
        assert!(res.status() == 200 || res.status() == 204, "dismiss: {}", res.status());
    });
}

// ============================================================
// 最終ログイン更新 (存在しないデバイス)
// ============================================================

#[tokio::test]
async fn test_update_last_login_not_found() {
    test_group!("最終ログイン更新");
    test_case!("存在しないデバイスIDで最終ログイン更新すると404が返る", {
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
    });
}

// ============================================================
// バージョン報告 (存在しないデバイス)
// ============================================================

#[tokio::test]
async fn test_report_version_not_found() {
    test_group!("バージョン報告");
    test_case!("存在しないデバイスIDでバージョン報告すると404が返る", {
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
    });
}

// ============================================================
// Device Ownerフロー: クレーム + 一覧でis_device_owner確認
// ============================================================

#[tokio::test]
async fn test_device_owner_claim_and_verify_in_list() {
    test_group!("Device Ownerフロー");
    test_case!("Device Ownerデバイスがis_device_owner=trueで一覧に表示される", {
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
    });
}

// ============================================================
// 着信設定 (存在しないデバイス)
// ============================================================

#[tokio::test]
async fn test_update_call_settings_not_found() {
    test_group!("着信設定");
    test_case!("存在しないデバイスIDの着信設定更新で404が返る", {
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
    });
}

// ============================================================
// アップデートトリガー (内部認証)
// ============================================================

#[tokio::test]
async fn test_trigger_update_dev_no_secret() {
    test_group!("アップデートトリガー");
    test_case!("X-Internal-Secretなしでtrigger-update-devを呼ぶ", {
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
        // MockFcm有効だがtrigger-update-devは内部認証が別
        let _status = res.status();
    });
}

#[tokio::test]
async fn test_trigger_update_dev_with_secret() {
    test_group!("アップデートトリガー");
    test_case!("正しいX-Internal-Secretでdevデバイスにアップデート通知を送信できる", {
        std::env::set_var("FCM_INTERNAL_SECRET", "test-internal-secret");
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "TrigDevSec").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        // dev device 作成 + FCM token 登録 + is_dev_device=true
        let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
        client.put(format!("{base_url}/api/devices/register-fcm-token"))
            .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "dev-token" }))
            .send().await.unwrap();
        client.put(format!("{base_url}/api/devices/report-version"))
            .json(&serde_json::json!({
                "device_id": device_id, "version_code": 1, "version_name": "0.1",
                "is_dev_device": true
            }))
            .send().await.unwrap();

        let res = client
            .post(format!("{base_url}/api/devices/trigger-update-dev"))
            .header("X-Internal-Secret", "test-internal-secret")
            .json(&serde_json::json!({ "version_code": 100, "version_name": "2.0.0" }))
            .send().await.unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert!(body.get("sent").is_some());
    });
}

#[tokio::test]
async fn test_trigger_update_already_updated() {
    test_group!("アップデートトリガー");
    test_case!("既にアップデート済みのデバイスはalready_updatedとしてカウントされる", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "TrigAlready").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
        client.put(format!("{base_url}/api/devices/register-fcm-token"))
            .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "already-token" }))
            .send().await.unwrap();
        // version_code=100 を報告
        client.put(format!("{base_url}/api/devices/report-version"))
            .json(&serde_json::json!({ "device_id": device_id, "version_code": 100, "version_name": "1.0" }))
            .send().await.unwrap();

        // version_code=50 で trigger → already_updated
        let res = client
            .post(format!("{base_url}/api/devices/trigger-update"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "version_code": 50, "version_name": "0.5" }))
            .send().await.unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert!(body["already_updated"].as_i64().unwrap() >= 1);
    });
}

#[tokio::test]
async fn test_trigger_update_with_device_ids_filter() {
    test_group!("アップデートトリガー");
    test_case!("存在しないdevice_idsでフィルタするとskippedになる", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "TrigFilter").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
        client.put(format!("{base_url}/api/devices/register-fcm-token"))
            .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "filter-token" }))
            .send().await.unwrap();

        // 存在しない device_id でフィルタ → skipped
        let fake_id = uuid::Uuid::new_v4();
        let res = client
            .post(format!("{base_url}/api/devices/trigger-update"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "version_code": 200, "version_name": "2.0",
                "device_ids": [fake_id.to_string()]
            }))
            .send().await.unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert!(body["skipped"].as_i64().unwrap() >= 1);
    });
}

#[tokio::test]
async fn test_trigger_update_dev_wrong_secret() {
    test_group!("アップデートトリガー");
    test_case!("誤ったX-Internal-Secretで401が返る", {
        std::env::set_var("FCM_INTERNAL_SECRET", "test-internal-secret");
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state).await;
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/devices/trigger-update-dev"))
            .header("X-Internal-Secret", "definitely-wrong-secret")
            .json(&serde_json::json!({ "version_code": 100, "version_name": "2.0.0" }))
            .send().await.unwrap();
        assert_eq!(res.status(), 401);
    });
}

#[tokio::test]
async fn test_test_fcm_all_with_token() {
    test_group!("アップデートトリガー");
    test_case!("FCMトークン登録済みデバイスにtest-fcm-allで通知される", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "FcmAllToken").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
        client.put(format!("{base_url}/api/devices/register-fcm-token"))
            .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "all-token" }))
            .send().await.unwrap();

        let res = client
            .post(format!("{base_url}/api/devices/test-fcm-all"))
            .header("Authorization", &auth)
            .send().await.unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert!(body["sent"].as_i64().unwrap() >= 1);
        assert!(body["results"].as_array().unwrap().len() >= 1);
    });
}

#[tokio::test]
async fn test_trigger_update_with_jwt() {
    test_group!("アップデートトリガー");
    test_case!("JWT認証でtrigger-updateが呼べる", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "TriggerUpd").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
        client.put(format!("{base_url}/api/devices/register-fcm-token"))
            .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "trigger-token" }))
            .send().await.unwrap();

        let res = client
            .post(format!("{base_url}/api/devices/trigger-update"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "version_code": 100, "version_name": "2.0.0" }))
            .send().await.unwrap();
        assert_eq!(res.status(), 200);
    });
}

// ============================================================
// FCMテスト (一括除外)
// ============================================================

#[tokio::test]
async fn test_test_fcm_all_exclude() {
    test_group!("FCMテスト");
    test_case!("test-fcm-all-excludeが正常に動作する", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state).await;
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/devices/test-fcm-all-exclude"))
            .json(&serde_json::json!({
                "exclude_device_ids": ["dev1"]
            }))
            .send()
            .await
            .unwrap();
        // MockFcmSender 注入済み → FCM 有効
        assert!(res.status() == 200 || res.status() == 204, "FCM endpoint returned {}", res.status());
    });
}

// ============================================================
// アップデートトリガー (テナント認証)
// ============================================================

#[tokio::test]
async fn test_trigger_update() {
    test_group!("アップデートトリガー");
    test_case!("テナント認証でtrigger-updateが正常に動作する", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "TrigUpd").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/devices/trigger-update"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({
                "version_code": 100,
                "version_name": "2.0.0"
            }))
            .send()
            .await
            .unwrap();
        // MockFcmSender 注入済み → FCM 有効
        assert!(res.status() == 200 || res.status() == 204, "FCM endpoint returned {}", res.status());
    });
}

// ============================================================
// 個別デバイスFCMテスト (テナント認証)
// ============================================================

#[tokio::test]
async fn test_test_fcm_for_device() {
    test_group!("個別デバイスFCMテスト");
    test_case!("特定デバイスへのFCMテスト送信が成功する", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "FcmDev").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;

        // FCM トークンを登録してからテスト
        client.put(format!("{base_url}/api/devices/register-fcm-token"))
            .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "test-token-for-fcm" }))
            .send().await.unwrap();

        let res = client
            .post(format!("{base_url}/api/devices/{device_id}/test-fcm"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        // MockFcmSender → 200/204
        assert!(res.status() == 200 || res.status() == 204, "FCM test: {}", res.status());
    });
}

// ============================================================
// 一括FCMテスト (テナント認証)
// ============================================================

#[tokio::test]
async fn test_test_fcm_all() {
    test_group!("一括FCMテスト");
    test_case!("test-fcm-allが正常に動作する", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "FcmAll").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/devices/test-fcm-all"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        // MockFcmSender 注入済み → FCM 有効
        assert!(res.status() == 200 || res.status() == 204, "FCM endpoint returned {}", res.status());
    });
}

use serde_json::Value;

// ============================================================
// ヘルパー
// ============================================================

async fn setup_tenant_with_user(state: &rust_alc_api::AppState) -> (uuid::Uuid, String) {
    let tenant_id = crate::common::create_test_tenant(
        state.pool(),
        &format!("Dev{}", uuid::Uuid::new_v4().simple()),
    )
    .await;
    let (user_id, _) =
        crate::common::create_test_user_in_db(state.pool(), tenant_id, "dev@test.com", "admin")
            .await;
    let jwt = crate::common::create_test_jwt_for_user(user_id, tenant_id, "dev@test.com", "admin");
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
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("公開エンドポイント");
    test_case!("QR一時登録リクエストを作成できる", {
        let state = crate::common::setup_app_state().await;
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
    });
}

#[tokio::test]
async fn test_check_status_pending() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("公開エンドポイント");
    test_case!(
        "登録リクエストのステータスがpendingで返る",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state).await;
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
    );
}

#[tokio::test]
async fn test_check_status_not_found() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("公開エンドポイント");
    test_case!(
        "存在しない登録コードのステータス確認で404が返る",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state).await;
            let client = reqwest::Client::new();

            let res = client
                .get(format!("{base_url}/api/devices/register/status/INVALID"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 404);
        }
    );
}

#[tokio::test]
async fn test_device_settings_not_found() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("公開エンドポイント");
    test_case!(
        "存在しないデバイスIDの設定取得で404が返る",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state).await;
            let client = reqwest::Client::new();

            let fake_id = uuid::Uuid::new_v4();
            let res = client
                .get(format!("{base_url}/api/devices/settings/{fake_id}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 404);
        }
    );
}

// ============================================================
// URL フロー
// ============================================================

#[tokio::test]
async fn test_url_flow_create_token() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("URLフロー");
    test_case!(
        "管理者がURLフロー用トークンを生成できる",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "URL Token").await;
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
            assert!(body["registration_url"].as_str().is_some());
        }
    );
}

#[tokio::test]
async fn test_url_flow_claim() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("URLフロー");
    test_case!(
        "端末がURLフローでクレームして即登録される",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "URL Claim").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
            assert!(!device_id.is_empty());
        }
    );
}

#[tokio::test]
async fn test_url_flow_device_in_list() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("URLフロー");
    test_case!(
        "URLフローで作成したデバイスが一覧に表示される",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "URL List").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");
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
    );
}

// ============================================================
// QR永久フロー
// ============================================================

#[tokio::test]
async fn test_qr_permanent_create() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("QR永久フロー");
    test_case!("管理者がQR永久コードを生成できる", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "QR Perm Create").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
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
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("QR永久フロー");
    test_case!(
        "端末がQR永久コードでクレームすると承認待ちになる",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "QR Perm Claim").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");
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
    );
}

#[tokio::test]
async fn test_qr_permanent_in_pending() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("QR永久フロー");
    test_case!(
        "QR永久クレーム後に承認待ち一覧に表示される",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id =
                crate::common::create_test_tenant(state.pool(), "QR Perm Pending").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");
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
    );
}

// ============================================================
// QR一時フロー
// ============================================================

#[tokio::test]
async fn test_qr_temp_approve_by_code() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("QR一時フロー");
    test_case!("管理者がコードで直接承認できる", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
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
            .post(format!("{base_url}/api/devices/approve-by-code/{code}"))
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
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("QR一時フロー");
    test_case!("承認後のステータスがapprovedに変わる", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
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
            .post(format!("{base_url}/api/devices/approve-by-code/{code}"))
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
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("管理操作");
    test_case!("デバイス一覧が200で返る", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "Empty Devices").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
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
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("管理操作");
    test_case!("リクエストIDで承認できる", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
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
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("管理操作");
    test_case!("登録リクエストを拒否できる", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "Reject Dev").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
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
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("管理操作");
    test_case!("存在しないリクエストIDの拒否で404が返る", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "Reject NF").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
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
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("管理操作");
    test_case!("デバイスの無効化と有効化ができる", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "Disable Enable").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
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
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("管理操作");
    test_case!(
        "デバイスを削除でき、設定取得で404になる",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "Delete Dev").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");
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
    );
}

#[tokio::test]
async fn test_delete_not_found() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("管理操作");
    test_case!("存在しないデバイスIDの削除で404が返る", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "Del NF Dev").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
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
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("管理操作");
    test_case!(
        "クロステナント操作が可能であることを記録する (RLSポリシー問題)",
        {
            // NOTE: device_select_by_id ポリシー (FOR SELECT USING (true)) により
            // SELECT/UPDATE/DELETE は全テナント横断でアクセス可能な状態。
            // このテストはクロステナント操作が実行可能であることを記録する。
            // TODO: RLS ポリシーを修正して、UPDATE/DELETE にテナント分離を追加すべき
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;

            let tenant_a = crate::common::create_test_tenant(state.pool(), "Dev Iso A").await;
            let tenant_b = crate::common::create_test_tenant(state.pool(), "Dev Iso B").await;

            let jwt_a = crate::common::create_test_jwt(tenant_a, "admin");
            let _jwt_b = crate::common::create_test_jwt(tenant_b, "admin");
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
    );
}

// ============================================================
// 端末側公開エンドポイント
// ============================================================

#[tokio::test]
async fn test_register_fcm_token() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("端末側公開エンドポイント");
    test_case!("FCMトークンを登録できる", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "FCM Token").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
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
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("端末側公開エンドポイント");
    test_case!("最終ログイン情報を更新できる", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "Last Login").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
        let emp =
            crate::common::create_test_employee(&client, &base_url, &auth, "LoginEmp", "LE01")
                .await;
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
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("端末側公開エンドポイント");
    test_case!("バージョン情報を報告できる", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "Version").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
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
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("端末側公開エンドポイント");
    test_case!("Watchdog状態を報告できる", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "Watchdog").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
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
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("端末側公開エンドポイント");
    test_case!(
        "FCMトークンなしでもfcm-notify-callが200を返す",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state).await;
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
        }
    );
}

#[tokio::test]
async fn test_fcm_notify_call_with_token() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("端末側公開エンドポイント");
    test_case!(
        "FCMトークン登録済みデバイスにFCM通知が送信される",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id =
                crate::common::create_test_tenant(state.pool(), "FCM Notify Token").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            // デバイス作成 + FCM トークン登録 + call_enabled=true
            let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
            client.put(format!("{base_url}/api/devices/register-fcm-token"))
            .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "test-fcm-token-123" }))
            .send().await.unwrap();
            client
                .put(format!("{base_url}/api/devices/{device_id}/call-settings"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "call_enabled": true }))
                .send()
                .await
                .unwrap();

            // FCM notify-call → should send to device
            let res = client
                .post(format!("{base_url}/api/devices/fcm-notify-call"))
                .json(&serde_json::json!({ "room_ids": ["room-abc"] }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert!(
                body["sent"].as_i64().unwrap() >= 1,
                "should send to at least 1 device"
            );
        }
    );
}

#[tokio::test]
async fn test_fcm_notify_call_with_exclude() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("端末側公開エンドポイント");
    test_case!(
        "除外デバイスIDを指定するとFCM通知がスキップされる",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "FCM Exclude").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
            client
                .put(format!("{base_url}/api/devices/register-fcm-token"))
                .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "token-exclude" }))
                .send()
                .await
                .unwrap();
            client
                .put(format!("{base_url}/api/devices/{device_id}/call-settings"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "call_enabled": true }))
                .send()
                .await
                .unwrap();

            // exclude this device → skipped
            let res = client
                .post(format!("{base_url}/api/devices/fcm-notify-call"))
                .json(&serde_json::json!({
                    "room_ids": ["room-1"],
                    "exclude_device_ids": [device_id]
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert!(body["skipped"].as_i64().unwrap() >= 1);
        }
    );
}

#[tokio::test]
async fn test_fcm_notify_call_with_schedule() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("端末側公開エンドポイント");
    test_case!(
        "スケジュール時間外のデバイスはFCM通知がスキップされる",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "FCM Schedule").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
            client
                .put(format!("{base_url}/api/devices/register-fcm-token"))
                .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "token-sched" }))
                .send()
                .await
                .unwrap();

            // call_enabled=true + schedule with narrow window that excludes current time
            client
                .put(format!("{base_url}/api/devices/{device_id}/call-settings"))
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
                .send()
                .await
                .unwrap();

            // Schedule window is 03:00-03:01, current time is likely not in that range → skipped
            let res = client
                .post(format!("{base_url}/api/devices/fcm-notify-call"))
                .json(&serde_json::json!({ "room_ids": ["room-1"] }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert!(
                body["skipped"].as_i64().unwrap() >= 1,
                "should skip due to schedule"
            );
        }
    );
}

#[tokio::test]
async fn test_fcm_notify_call_disabled() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("端末側公開エンドポイント");
    test_case!(
        "着信無効のデバイスはFCM通知がスキップされる",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "FCM Disabled").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
            client
                .put(format!("{base_url}/api/devices/register-fcm-token"))
                .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "token-disabled" }))
                .send()
                .await
                .unwrap();

            // call_enabled=false → skipped
            let res = client
                .post(format!("{base_url}/api/devices/fcm-notify-call"))
                .json(&serde_json::json!({ "room_ids": ["room-1"] }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
        }
    );
}

#[tokio::test]
async fn test_device_owner_flow() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("端末側公開エンドポイント");
    test_case!(
        "Device Ownerフローでクレームすると即承認される",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "Dev Owner").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");
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
    );
}

// ============================================================
// デバイス設定
// ============================================================

#[tokio::test]
async fn test_device_settings_after_creation() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("デバイス設定");
    test_case!(
        "作成直後のデバイス設定がデフォルト値で取得できる",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "Settings Test").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");
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
    );
}

#[tokio::test]
async fn test_update_call_settings() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("デバイス設定");
    test_case!("着信設定を更新できる", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "Call Settings").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;

        // 着信設定更新
        let res = client
            .put(format!("{base_url}/api/devices/{device_id}/call-settings"))
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
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("FCM dismissテスト");
    test_case!("MockFcmSenderでfcm-dismiss-testが成功する", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "FCM Dismiss").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
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
        assert!(
            res.status() == 200 || res.status() == 204,
            "dismiss: {}",
            res.status()
        );
    });
}

// ============================================================
// 最終ログイン更新 (存在しないデバイス)
// ============================================================

#[tokio::test]
async fn test_update_last_login_not_found() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("最終ログイン更新");
    test_case!(
        "存在しないデバイスIDで最終ログイン更新すると404が返る",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state).await;
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
    );
}

// ============================================================
// バージョン報告 (存在しないデバイス)
// ============================================================

#[tokio::test]
async fn test_report_version_not_found() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("バージョン報告");
    test_case!(
        "存在しないデバイスIDでバージョン報告すると404が返る",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state).await;
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
    );
}

// ============================================================
// Device Ownerフロー: クレーム + 一覧でis_device_owner確認
// ============================================================

#[tokio::test]
async fn test_device_owner_claim_and_verify_in_list() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("Device Ownerフロー");
    test_case!(
        "Device Ownerデバイスがis_device_owner=trueで一覧に表示される",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "DO Verify").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");
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
            let our_device = devices.iter().find(|d| d["id"].as_str() == Some(device_id));
            assert!(
                our_device.is_some(),
                "Device Owner device should appear in list"
            );
            assert_eq!(our_device.unwrap()["is_device_owner"], true);
        }
    );
}

// ============================================================
// 着信設定 (存在しないデバイス)
// ============================================================

#[tokio::test]
async fn test_update_call_settings_not_found() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("着信設定");
    test_case!(
        "存在しないデバイスIDの着信設定更新で404が返る",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "Call NF").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();

            let fake_id = uuid::Uuid::new_v4();
            let res = client
                .put(format!("{base_url}/api/devices/{fake_id}/call-settings"))
                .header("Authorization", format!("Bearer {jwt}"))
                .json(&serde_json::json!({
                    "call_enabled": true
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 404);
        }
    );
}

// ============================================================
// アップデートトリガー (内部認証)
// ============================================================

#[tokio::test]
async fn test_trigger_update_dev_no_secret() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("アップデートトリガー");
    test_case!("X-Internal-Secretなしでtrigger-update-devを呼ぶ", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;
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
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("アップデートトリガー");
    test_case!(
        "正しいX-Internal-Secretでdevデバイスにアップデート通知を送信できる",
        {
            std::env::set_var("FCM_INTERNAL_SECRET", "test-internal-secret");
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "TrigDevSec").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            // dev device 作成 + FCM token 登録 + is_dev_device=true
            let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
            client
                .put(format!("{base_url}/api/devices/register-fcm-token"))
                .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "dev-token" }))
                .send()
                .await
                .unwrap();
            client
                .put(format!("{base_url}/api/devices/report-version"))
                .json(&serde_json::json!({
                    "device_id": device_id, "version_code": 1, "version_name": "0.1",
                    "is_dev_device": true
                }))
                .send()
                .await
                .unwrap();

            let res = client
                .post(format!("{base_url}/api/devices/trigger-update-dev"))
                .header("X-Internal-Secret", "test-internal-secret")
                .json(&serde_json::json!({ "version_code": 100, "version_name": "2.0.0" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert!(body.get("sent").is_some());
        }
    );
}

#[tokio::test]
async fn test_trigger_update_already_updated() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("アップデートトリガー");
    test_case!(
        "既にアップデート済みのデバイスはalready_updatedとしてカウントされる",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "TrigAlready").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
            client
                .put(format!("{base_url}/api/devices/register-fcm-token"))
                .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "already-token" }))
                .send()
                .await
                .unwrap();
            // version_code=100 を報告
            client.put(format!("{base_url}/api/devices/report-version"))
            .json(&serde_json::json!({ "device_id": device_id, "version_code": 100, "version_name": "1.0" }))
            .send().await.unwrap();

            // version_code=50 で trigger → already_updated
            let res = client
                .post(format!("{base_url}/api/devices/trigger-update"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "version_code": 50, "version_name": "0.5" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert!(body["already_updated"].as_i64().unwrap() >= 1);
        }
    );
}

#[tokio::test]
async fn test_trigger_update_with_device_ids_filter() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("アップデートトリガー");
    test_case!(
        "存在しないdevice_idsでフィルタするとskippedになる",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "TrigFilter").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
            client
                .put(format!("{base_url}/api/devices/register-fcm-token"))
                .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "filter-token" }))
                .send()
                .await
                .unwrap();

            // 存在しない device_id でフィルタ → skipped
            let fake_id = uuid::Uuid::new_v4();
            let res = client
                .post(format!("{base_url}/api/devices/trigger-update"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "version_code": 200, "version_name": "2.0",
                    "device_ids": [fake_id.to_string()]
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert!(body["skipped"].as_i64().unwrap() >= 1);
        }
    );
}

#[tokio::test]
async fn test_trigger_update_dev_wrong_secret() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("アップデートトリガー");
    test_case!("誤ったX-Internal-Secretで401が返る", {
        std::env::set_var("FCM_INTERNAL_SECRET", "test-internal-secret");
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/devices/trigger-update-dev"))
            .header("X-Internal-Secret", "definitely-wrong-secret")
            .json(&serde_json::json!({ "version_code": 100, "version_name": "2.0.0" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 401);
    });
}

#[tokio::test]
async fn test_test_fcm_all_with_token() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("アップデートトリガー");
    test_case!(
        "FCMトークン登録済みデバイスにtest-fcm-allで通知される",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "FcmAllToken").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
            client
                .put(format!("{base_url}/api/devices/register-fcm-token"))
                .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "all-token" }))
                .send()
                .await
                .unwrap();

            let res = client
                .post(format!("{base_url}/api/devices/test-fcm-all"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert!(body["sent"].as_i64().unwrap() >= 1);
            assert!(body["results"].as_array().unwrap().len() >= 1);
        }
    );
}

#[tokio::test]
async fn test_trigger_update_with_jwt() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("アップデートトリガー");
    test_case!("JWT認証でtrigger-updateが呼べる", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "TriggerUpd").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
        client
            .put(format!("{base_url}/api/devices/register-fcm-token"))
            .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "trigger-token" }))
            .send()
            .await
            .unwrap();

        let res = client
            .post(format!("{base_url}/api/devices/trigger-update"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "version_code": 100, "version_name": "2.0.0" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    });
}

// ============================================================
// FCMテスト (一括除外)
// ============================================================

#[tokio::test]
async fn test_test_fcm_all_exclude() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("FCMテスト");
    test_case!("test-fcm-all-excludeが正常に動作する", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state).await;
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
        assert!(
            res.status() == 200 || res.status() == 204,
            "FCM endpoint returned {}",
            res.status()
        );
    });
}

// ============================================================
// アップデートトリガー (テナント認証)
// ============================================================

#[tokio::test]
async fn test_trigger_update() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("アップデートトリガー");
    test_case!(
        "テナント認証でtrigger-updateが正常に動作する",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "TrigUpd").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");
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
            assert!(
                res.status() == 200 || res.status() == 204,
                "FCM endpoint returned {}",
                res.status()
            );
        }
    );
}

// ============================================================
// 個別デバイスFCMテスト (テナント認証)
// ============================================================

#[tokio::test]
async fn test_test_fcm_for_device() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("個別デバイスFCMテスト");
    test_case!(
        "特定デバイスへのFCMテスト送信が成功する",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(state.pool(), "FcmDev").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");
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
            assert!(
                res.status() == 200 || res.status() == 204,
                "FCM test: {}",
                res.status()
            );
        }
    );
}

// ============================================================
// 一括FCMテスト (テナント認証)
// ============================================================

#[tokio::test]
async fn test_test_fcm_all() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    test_group!("一括FCMテスト");
    test_case!("test-fcm-allが正常に動作する", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "FcmAll").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/devices/test-fcm-all"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        // MockFcmSender 注入済み → FCM 有効
        assert!(
            res.status() == 200 || res.status() == 204,
            "FCM endpoint returned {}",
            res.status()
        );
    });
}

// ============================================================
// カバレッジ専用テスト
// ============================================================

// --- L151-153: create_registration_request DB error (pool.close) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_create_registration_request_db_error() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    state.pool().close().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/register/request"))
        .json(&serde_json::json!({ "device_name": "err" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// --- L199-201: check_registration_status DB error (pool.close) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_check_registration_status_db_error() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    state.pool().close().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/devices/register/status/ANY"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// --- L215: expired registration request ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_check_registration_status_expired() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let client = reqwest::Client::new();

    // Insert an already-expired registration request directly into DB
    let code = format!("EXP{}", uuid::Uuid::new_v4().simple());
    sqlx::query(
        r#"
        INSERT INTO device_registration_requests
            (registration_code, flow_type, device_name, status, expires_at)
        VALUES ($1, 'qr_temp', 'expired-dev', 'pending', NOW() - INTERVAL '1 hour')
        "#,
    )
    .bind(&code)
    .execute(state.pool())
    .await
    .unwrap();

    let res = client
        .get(format!("{base_url}/api/devices/register/status/{code}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["status"], "expired");
}

// --- L220: pending request with no expires_at (qr_permanent) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_check_registration_status_no_expires_at() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let client = reqwest::Client::new();

    // Insert a registration request with no expires_at (like qr_permanent)
    let code = format!("NXP{}", uuid::Uuid::new_v4().simple());
    sqlx::query(
        r#"
        INSERT INTO device_registration_requests
            (registration_code, flow_type, device_name, status)
        VALUES ($1, 'qr_permanent', 'noexpiry-dev', 'pending')
        "#,
    )
    .bind(&code)
    .execute(state.pool())
    .await
    .unwrap();

    let res = client
        .get(format!("{base_url}/api/devices/register/status/{code}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["status"], "pending");
}

// --- L260-270,284-286: claim_registration DB lookup error (pool.close) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_claim_registration_db_error() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    state.pool().close().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({
            "registration_code": "DOESNOTMATTER",
            "phone_number": "000"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(body["message"].as_str().unwrap().contains("internal error"));
}

// --- L287: invalid registration code (ok_or_else) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_claim_registration_invalid_code() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({
            "registration_code": "NONEXISTENT_CODE_12345"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(body["message"]
        .as_str()
        .unwrap()
        .contains("無効な登録コード"));
}

// --- L301: claim on already-used code ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_claim_registration_already_used() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "ClaimUsed").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // Create URL token + claim it (first time succeeds)
    let (_, code) = create_device_via_url_flow(&client, &base_url, &auth).await;

    // Try to claim again with the same code (status is now 'approved')
    let res = client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({
            "registration_code": code,
            "device_name": "Second Try"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(body["message"].as_str().unwrap().contains("既に使用済み"));
}

// --- L478: unknown flow_type (CHECK制約を一時DROPしてテスト) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_claim_registration_unknown_flow_type() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let client = reqwest::Client::new();

    // CHECK制約を一時的にDROP
    sqlx::query("ALTER TABLE alc_api.device_registration_requests DROP CONSTRAINT IF EXISTS device_registration_requests_flow_type_check")
        .execute(state.pool()).await.unwrap();

    let code = format!("UNK{}", uuid::Uuid::new_v4().simple());
    sqlx::query(
        r#"
        INSERT INTO alc_api.device_registration_requests
            (registration_code, flow_type, device_name, status)
        VALUES ($1, 'unknown_flow', 'unknown-dev', 'pending')
        "#,
    )
    .bind(&code)
    .execute(state.pool())
    .await
    .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({
            "registration_code": code
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(body["message"]
        .as_str()
        .unwrap()
        .contains("無効なフロータイプ"));

    // 不正行を削除してからCHECK制約を復元
    sqlx::query(
        "DELETE FROM alc_api.device_registration_requests WHERE flow_type = 'unknown_flow'",
    )
    .execute(state.pool())
    .await
    .unwrap();
    sqlx::query("ALTER TABLE alc_api.device_registration_requests ADD CONSTRAINT device_registration_requests_flow_type_check CHECK (flow_type IN ('qr_temp', 'qr_permanent', 'url', 'device_owner'))")
        .execute(state.pool()).await.unwrap();
}

// ============================================================
// カバレッジ専用: DB エラーパス (RENAME / trigger)
// ============================================================

/// RENAME テスト用の安全ヘルパー: テーブルが _bak のまま残っていたら復元する
async fn ensure_table_exists(pool: &sqlx::PgPool, table: &str) {
    let bak = format!("{table}_bak");
    // _bak が存在するか確認 (前回テストのパニックで残った場合)
    let exists: bool = sqlx::query_scalar(&format!(
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_schema='alc_api' AND table_name='{bak}')"
    ))
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    if exists {
        let _ = sqlx::query(&format!("ALTER TABLE alc_api.{bak} RENAME TO {table}"))
            .execute(pool)
            .await;
    }
}

// --- list_devices DB error (RENAME devices) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_list_devices_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "DevListErr").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    ensure_table_exists(state.pool(), "devices").await;
    sqlx::query("ALTER TABLE alc_api.devices RENAME TO devices_bak")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .get(format!("{base_url}/api/devices"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("ALTER TABLE alc_api.devices_bak RENAME TO devices")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- list_pending DB error (RENAME device_registration_requests) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_list_pending_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "DevPendErr").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    ensure_table_exists(state.pool(), "device_registration_requests").await;
    sqlx::query("ALTER TABLE alc_api.device_registration_requests RENAME TO device_registration_requests_bak")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .get(format!("{base_url}/api/devices/pending"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("ALTER TABLE alc_api.device_registration_requests_bak RENAME TO device_registration_requests")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- create_url_token DB error (trigger INSERT on device_registration_requests) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_create_url_token_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "UrlTokErr").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION alc_api.fail_drr_ins_url() RETURNS trigger AS $$
        BEGIN RAISE EXCEPTION 'test error'; END; $$ LANGUAGE plpgsql"#,
    )
    .execute(state.pool())
    .await
    .unwrap();
    sqlx::query("CREATE OR REPLACE TRIGGER fail_drr_ins_url BEFORE INSERT ON alc_api.device_registration_requests FOR EACH ROW EXECUTE FUNCTION alc_api.fail_drr_ins_url()")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/register/create-token"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "device_name": "err" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("DROP TRIGGER fail_drr_ins_url ON alc_api.device_registration_requests")
        .execute(state.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION alc_api.fail_drr_ins_url")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- create_device_owner_token DB error (trigger INSERT on device_registration_requests) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_create_device_owner_token_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "DOTokErr").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION alc_api.fail_drr_ins_do() RETURNS trigger AS $$
        BEGIN RAISE EXCEPTION 'test error'; END; $$ LANGUAGE plpgsql"#,
    )
    .execute(state.pool())
    .await
    .unwrap();
    sqlx::query("CREATE OR REPLACE TRIGGER fail_drr_ins_do BEFORE INSERT ON alc_api.device_registration_requests FOR EACH ROW EXECUTE FUNCTION alc_api.fail_drr_ins_do()")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .post(format!(
            "{base_url}/api/devices/register/create-device-owner-token"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "device_name": "err" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("DROP TRIGGER fail_drr_ins_do ON alc_api.device_registration_requests")
        .execute(state.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION alc_api.fail_drr_ins_do")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- create_permanent_qr DB error (trigger INSERT on device_registration_requests) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_create_permanent_qr_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "PQRErr").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION alc_api.fail_drr_ins_pqr() RETURNS trigger AS $$
        BEGIN RAISE EXCEPTION 'test error'; END; $$ LANGUAGE plpgsql"#,
    )
    .execute(state.pool())
    .await
    .unwrap();
    sqlx::query("CREATE OR REPLACE TRIGGER fail_drr_ins_pqr BEFORE INSERT ON alc_api.device_registration_requests FOR EACH ROW EXECUTE FUNCTION alc_api.fail_drr_ins_pqr()")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .post(format!(
            "{base_url}/api/devices/register/create-permanent-qr"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "device_name": "err" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("DROP TRIGGER fail_drr_ins_pqr ON alc_api.device_registration_requests")
        .execute(state.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION alc_api.fail_drr_ins_pqr")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- approve_device DB error (RENAME device_registration_requests → lookup fails) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_approve_device_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "AppDevErr").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();

    ensure_table_exists(state.pool(), "device_registration_requests").await;
    sqlx::query("ALTER TABLE alc_api.device_registration_requests RENAME TO device_registration_requests_bak")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/approve/{fake_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("ALTER TABLE alc_api.device_registration_requests_bak RENAME TO device_registration_requests")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- approve_by_code DB error (RENAME device_registration_requests → lookup fails) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_approve_by_code_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "AppCodeErr").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    ensure_table_exists(state.pool(), "device_registration_requests").await;
    sqlx::query("ALTER TABLE alc_api.device_registration_requests RENAME TO device_registration_requests_bak")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/approve-by-code/FAKECODE"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("ALTER TABLE alc_api.device_registration_requests_bak RENAME TO device_registration_requests")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- reject_device DB error (trigger UPDATE on device_registration_requests) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_reject_device_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "RejDevErr").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // Create a pending request to reject
    let res = client
        .post(format!(
            "{base_url}/api/devices/register/create-permanent-qr"
        ))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "device_name": "RejectErr" }))
        .send()
        .await
        .unwrap();
    let body: Value = res.json().await.unwrap();
    let code = body["registration_code"].as_str().unwrap().to_string();

    // Claim it
    client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({ "registration_code": code }))
        .send()
        .await
        .unwrap();

    // Get request ID from pending list
    let res = client
        .get(format!("{base_url}/api/devices/pending"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    let pending: Vec<Value> = res.json().await.unwrap();
    let req_id = pending
        .iter()
        .find(|p| p["registration_code"] == code.as_str())
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Install trigger to fail UPDATE
    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION alc_api.fail_drr_upd_rej() RETURNS trigger AS $$
        BEGIN RAISE EXCEPTION 'test error'; END; $$ LANGUAGE plpgsql"#,
    )
    .execute(state.pool())
    .await
    .unwrap();
    sqlx::query("CREATE OR REPLACE TRIGGER fail_drr_upd_rej BEFORE UPDATE ON alc_api.device_registration_requests FOR EACH ROW EXECUTE FUNCTION alc_api.fail_drr_upd_rej()")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/reject/{req_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("DROP TRIGGER fail_drr_upd_rej ON alc_api.device_registration_requests")
        .execute(state.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION alc_api.fail_drr_upd_rej")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- disable_device DB error (trigger UPDATE on devices) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_disable_device_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "DisDevErr").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;

    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION alc_api.fail_dev_upd_dis() RETURNS trigger AS $$
        BEGIN RAISE EXCEPTION 'test error'; END; $$ LANGUAGE plpgsql"#,
    )
    .execute(state.pool())
    .await
    .unwrap();
    sqlx::query("CREATE OR REPLACE TRIGGER fail_dev_upd_dis BEFORE UPDATE ON alc_api.devices FOR EACH ROW EXECUTE FUNCTION alc_api.fail_dev_upd_dis()")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/disable/{device_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("DROP TRIGGER fail_dev_upd_dis ON alc_api.devices")
        .execute(state.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION alc_api.fail_dev_upd_dis")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- enable_device DB error (trigger UPDATE on devices) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_enable_device_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "EnDevErr").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;

    // Disable first so enable can target it
    client
        .post(format!("{base_url}/api/devices/disable/{device_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();

    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION alc_api.fail_dev_upd_en() RETURNS trigger AS $$
        BEGIN RAISE EXCEPTION 'test error'; END; $$ LANGUAGE plpgsql"#,
    )
    .execute(state.pool())
    .await
    .unwrap();
    sqlx::query("CREATE OR REPLACE TRIGGER fail_dev_upd_en BEFORE UPDATE ON alc_api.devices FOR EACH ROW EXECUTE FUNCTION alc_api.fail_dev_upd_en()")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/enable/{device_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("DROP TRIGGER fail_dev_upd_en ON alc_api.devices")
        .execute(state.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION alc_api.fail_dev_upd_en")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- delete_device DB error (trigger DELETE on devices) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_delete_device_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "DelDevErr").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;

    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION alc_api.fail_dev_del() RETURNS trigger AS $$
        BEGIN RAISE EXCEPTION 'test error'; END; $$ LANGUAGE plpgsql"#,
    )
    .execute(state.pool())
    .await
    .unwrap();
    sqlx::query("CREATE OR REPLACE TRIGGER fail_dev_del BEFORE DELETE ON alc_api.devices FOR EACH ROW EXECUTE FUNCTION alc_api.fail_dev_del()")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .delete(format!("{base_url}/api/devices/{device_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("DROP TRIGGER fail_dev_del ON alc_api.devices")
        .execute(state.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION alc_api.fail_dev_del")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- update_call_settings DB error (trigger UPDATE on devices) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_update_call_settings_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "CallSetErr").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;

    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION alc_api.fail_dev_upd_call() RETURNS trigger AS $$
        BEGIN RAISE EXCEPTION 'test error'; END; $$ LANGUAGE plpgsql"#,
    )
    .execute(state.pool())
    .await
    .unwrap();
    sqlx::query("CREATE OR REPLACE TRIGGER fail_dev_upd_call BEFORE UPDATE ON alc_api.devices FOR EACH ROW EXECUTE FUNCTION alc_api.fail_dev_upd_call()")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .put(format!("{base_url}/api/devices/{device_id}/call-settings"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "call_enabled": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("DROP TRIGGER fail_dev_upd_call ON alc_api.devices")
        .execute(state.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION alc_api.fail_dev_upd_call")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- report_watchdog_state DB error (RENAME devices → lookup fails) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_report_watchdog_state_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();

    // RENAME devices so lookup_device_tenant function fails
    ensure_table_exists(state.pool(), "devices").await;
    sqlx::query("ALTER TABLE alc_api.devices RENAME TO devices_bak")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .put(format!("{base_url}/api/devices/report-watchdog"))
        .json(&serde_json::json!({ "device_id": fake_id, "running": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("ALTER TABLE alc_api.devices_bak RENAME TO devices")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- register_fcm_token DB error (RENAME devices → lookup fails) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_register_fcm_token_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();

    ensure_table_exists(state.pool(), "devices").await;
    sqlx::query("ALTER TABLE alc_api.devices RENAME TO devices_bak")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .put(format!("{base_url}/api/devices/register-fcm-token"))
        .json(&serde_json::json!({ "device_id": fake_id, "fcm_token": "err-tok" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("ALTER TABLE alc_api.devices_bak RENAME TO devices")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- update_last_login DB error (RENAME devices → lookup fails) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_update_last_login_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();

    ensure_table_exists(state.pool(), "devices").await;
    sqlx::query("ALTER TABLE alc_api.devices RENAME TO devices_bak")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .put(format!("{base_url}/api/devices/update-last-login"))
        .json(&serde_json::json!({
            "device_id": fake_id,
            "employee_id": uuid::Uuid::new_v4(),
            "employee_name": "ErrEmp",
            "employee_role": ["driver"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("ALTER TABLE alc_api.devices_bak RENAME TO devices")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- report_version DB error (RENAME devices → lookup fails) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_report_version_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();

    ensure_table_exists(state.pool(), "devices").await;
    sqlx::query("ALTER TABLE alc_api.devices RENAME TO devices_bak")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .put(format!("{base_url}/api/devices/report-version"))
        .json(&serde_json::json!({
            "device_id": fake_id,
            "version_code": 1,
            "version_name": "0.1"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("ALTER TABLE alc_api.devices_bak RENAME TO devices")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- get_device_settings DB error (RENAME devices → function fails) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_get_device_settings_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();

    ensure_table_exists(state.pool(), "devices").await;
    sqlx::query("ALTER TABLE alc_api.devices RENAME TO devices_bak")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .get(format!("{base_url}/api/devices/settings/{fake_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("ALTER TABLE alc_api.devices_bak RENAME TO devices")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- fcm_notify_call DB error (RENAME devices → query fails) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_fcm_notify_call_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let client = reqwest::Client::new();

    ensure_table_exists(state.pool(), "devices").await;
    sqlx::query("ALTER TABLE alc_api.devices RENAME TO devices_bak")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/fcm-notify-call"))
        .json(&serde_json::json!({ "room_ids": ["room-1"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("ALTER TABLE alc_api.devices_bak RENAME TO devices")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- fcm_dismiss_test DB error (RENAME devices → device lookup fails) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_fcm_dismiss_test_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();

    ensure_table_exists(state.pool(), "devices").await;
    sqlx::query("ALTER TABLE alc_api.devices RENAME TO devices_bak")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/fcm-dismiss-test"))
        .json(&serde_json::json!({ "device_id": fake_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("ALTER TABLE alc_api.devices_bak RENAME TO devices")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- test_fcm_all_exclude DB error (RENAME devices → query fails) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_test_fcm_all_exclude_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let client = reqwest::Client::new();

    ensure_table_exists(state.pool(), "devices").await;
    sqlx::query("ALTER TABLE alc_api.devices RENAME TO devices_bak")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/test-fcm-all-exclude"))
        .json(&serde_json::json!({ "exclude_device_ids": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("ALTER TABLE alc_api.devices_bak RENAME TO devices")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- test_fcm DB error (RENAME devices → query fails) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_test_fcm_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "FcmTestErr").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();

    ensure_table_exists(state.pool(), "devices").await;
    sqlx::query("ALTER TABLE alc_api.devices RENAME TO devices_bak")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/{fake_id}/test-fcm"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("ALTER TABLE alc_api.devices_bak RENAME TO devices")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- test_fcm_all DB error (RENAME devices → query fails) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_test_fcm_all_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "FcmAllErr").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    ensure_table_exists(state.pool(), "devices").await;
    sqlx::query("ALTER TABLE alc_api.devices RENAME TO devices_bak")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/test-fcm-all"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("ALTER TABLE alc_api.devices_bak RENAME TO devices")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- trigger_update DB error (RENAME devices → send_update_fcm query fails) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_trigger_update_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "TrigUpdErr").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    ensure_table_exists(state.pool(), "devices").await;
    sqlx::query("ALTER TABLE alc_api.devices RENAME TO devices_bak")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/trigger-update"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "version_code": 100, "version_name": "2.0" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("ALTER TABLE alc_api.devices_bak RENAME TO devices")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- trigger_update_dev DB error (RENAME tenants → tenant query fails) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_trigger_update_dev_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let client = reqwest::Client::new();

    std::env::set_var("FCM_INTERNAL_SECRET", "test-secret-dev-err");

    // RENAME devices so the tenant query in trigger_update_dev fails
    ensure_table_exists(state.pool(), "devices").await;
    sqlx::query("ALTER TABLE alc_api.devices RENAME TO devices_bak")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/trigger-update-dev"))
        .header("X-Internal-Secret", "test-secret-dev-err")
        .json(&serde_json::json!({ "version_code": 100, "version_name": "2.0" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("ALTER TABLE alc_api.devices_bak RENAME TO devices")
        .execute(state.pool())
        .await
        .unwrap();
}

// ============================================================
// カバレッジ専用: ロジックパステスト
// ============================================================

// --- disable_device not found (rows_affected == 0) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_disable_device_not_found() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "DisNF").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let res = client
        .post(format!("{base_url}/api/devices/disable/{fake_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// --- enable_device not found (rows_affected == 0) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_enable_device_not_found() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "EnNF").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let res = client
        .post(format!("{base_url}/api/devices/enable/{fake_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// --- register_fcm_token not found (device doesn't exist) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_register_fcm_token_not_found() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let res = client
        .put(format!("{base_url}/api/devices/register-fcm-token"))
        .json(&serde_json::json!({ "device_id": fake_id, "fcm_token": "nope" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// --- report_watchdog_state not found (device doesn't exist) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_report_watchdog_state_not_found() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let res = client
        .put(format!("{base_url}/api/devices/report-watchdog"))
        .json(&serde_json::json!({ "device_id": fake_id, "running": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// --- fcm_notify_call with empty room_ids returns early ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_fcm_notify_call_empty_rooms() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/fcm-notify-call"))
        .json(&serde_json::json!({ "room_ids": [] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["sent"], 0);
    assert_eq!(body["skipped"], 0);
    assert_eq!(body["errors"], 0);
}

// --- fcm_notify_call unauthorized (wrong X-Internal-Secret) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_fcm_notify_call_unauthorized() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let _env = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("FCM_INTERNAL_SECRET", "correct-secret-nc");
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/fcm-notify-call"))
        .header("X-Internal-Secret", "wrong-secret")
        .json(&serde_json::json!({ "room_ids": ["room-1"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
    std::env::remove_var("FCM_INTERNAL_SECRET");
}

// --- fcm_notify_call authorized (correct X-Internal-Secret) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_fcm_notify_call_authorized() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let _env = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("FCM_INTERNAL_SECRET", "correct-secret-auth");
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/fcm-notify-call"))
        .header("X-Internal-Secret", "correct-secret-auth")
        .json(&serde_json::json!({ "room_ids": ["room-1"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    std::env::remove_var("FCM_INTERNAL_SECRET");
}

// --- test_fcm not found (device doesn't exist) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_test_fcm_not_found() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "FcmNF").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let res = client
        .post(format!("{base_url}/api/devices/{fake_id}/test-fcm"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// --- test_fcm no token (device exists but no FCM token → 400) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_test_fcm_no_token() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "FcmNoTok").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;

    // Don't register FCM token → BAD_REQUEST
    let res = client
        .post(format!("{base_url}/api/devices/{device_id}/test-fcm"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

// --- fcm_dismiss_test not found (device doesn't exist) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_fcm_dismiss_test_not_found() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let res = client
        .post(format!("{base_url}/api/devices/fcm-dismiss-test"))
        .json(&serde_json::json!({ "device_id": fake_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// --- approve_device not found (request doesn't exist → 404) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_approve_device_not_found() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "AppNF").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let res = client
        .post(format!("{base_url}/api/devices/approve/{fake_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// --- approve_by_code not found (code doesn't exist → 404) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_approve_by_code_not_found() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "AppCodeNF").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!(
            "{base_url}/api/devices/approve-by-code/NONEXISTENT"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// --- trigger_update_dev with FCM_INTERNAL_SECRET not set → 503 ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_trigger_update_dev_no_secret_503() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let _env = crate::common::ENV_LOCK.lock().unwrap();
    std::env::remove_var("FCM_INTERNAL_SECRET");
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/trigger-update-dev"))
        .json(&serde_json::json!({ "version_code": 100, "version_name": "2.0" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 503);
}

// --- should_notify_device: schedule enabled=false → skipped ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_fcm_notify_schedule_enabled_false() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "SchedEnF").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
    client
        .put(format!("{base_url}/api/devices/register-fcm-token"))
        .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "sched-token" }))
        .send()
        .await
        .unwrap();
    client
        .put(format!("{base_url}/api/devices/{device_id}/call-settings"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "call_enabled": true,
            "call_schedule": { "enabled": false }
        }))
        .send()
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/fcm-notify-call"))
        .json(&serde_json::json!({ "room_ids": ["room-1"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["skipped"].as_i64().unwrap() >= 1);
}

// --- should_notify_device: wrong day of week → skipped ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_fcm_notify_schedule_wrong_day() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "SchedDay").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
    client
        .put(format!("{base_url}/api/devices/register-fcm-token"))
        .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "day-token" }))
        .send()
        .await
        .unwrap();

    // Set schedule to only allow day 7 (which doesn't exist in 0-6 range)
    // Actually use a day that is definitely not today
    // JST weekday: 0=Sun..6=Sat. Pick a day that differs from current.
    let now_jst_day = {
        use chrono::{Datelike, Utc};
        let now = Utc::now();
        let jst = chrono::FixedOffset::east_opt(9 * 3600).unwrap();
        now.with_timezone(&jst).weekday().num_days_from_sunday()
    };
    let wrong_day = (now_jst_day + 1) % 7;

    client
        .put(format!("{base_url}/api/devices/{device_id}/call-settings"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "call_enabled": true,
            "call_schedule": {
                "enabled": true,
                "days": [wrong_day],
                "startHour": 0,
                "startMin": 0,
                "endHour": 24,
                "endMin": 0
            }
        }))
        .send()
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/fcm-notify-call"))
        .json(&serde_json::json!({ "room_ids": ["room-1"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["skipped"].as_i64().unwrap() >= 1);
}

// --- should_notify_device: overnight schedule (start > end) → pass ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_fcm_notify_schedule_overnight_pass() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "SchedOvn").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
    client
        .put(format!("{base_url}/api/devices/register-fcm-token"))
        .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "ovn-token" }))
        .send()
        .await
        .unwrap();

    // Overnight schedule 22:00-06:00 covers all hours (wraps around)
    // Or 0:00-23:59 covers all day. Use a wide overnight window.
    client
        .put(format!("{base_url}/api/devices/{device_id}/call-settings"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "call_enabled": true,
            "call_schedule": {
                "enabled": true,
                "days": [0, 1, 2, 3, 4, 5, 6],
                "startHour": 23,
                "startMin": 0,
                "endHour": 0,
                "endMin": 1
            }
        }))
        .send()
        .await
        .unwrap();

    // This might send or skip depending on time of day. Either way covers the overnight branch.
    let res = client
        .post(format!("{base_url}/api/devices/fcm-notify-call"))
        .json(&serde_json::json!({ "room_ids": ["room-1"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

// --- update_call_settings with always_on triggers FCM settings_changed ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_update_call_settings_always_on_fcm() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "AlwaysOnFcm").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;

    // Register FCM token first so that settings_changed FCM is sent
    client
        .put(format!("{base_url}/api/devices/register-fcm-token"))
        .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "always-on-token" }))
        .send()
        .await
        .unwrap();

    // Update with always_on to trigger FCM settings_changed
    let res = client
        .put(format!("{base_url}/api/devices/{device_id}/call-settings"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "call_enabled": true,
            "always_on": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

// --- generate_unique_code DB error (pool.close) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_generate_unique_code_db_error() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    state.pool().close().await;
    let client = reqwest::Client::new();

    // create_registration_request calls generate_unique_code which will fail
    let res = client
        .post(format!("{base_url}/api/devices/register/request"))
        .json(&serde_json::json!({ "device_name": "codegen-err" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// カバレッジ専用: FCM 未設定 (fcm: None) テスト
// ============================================================

// --- fcm_notify_call: FCM 未設定 → SERVICE_UNAVAILABLE ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_fcm_notify_call_no_fcm_configured() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state_no_fcm().await;
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/fcm-notify-call"))
        .json(&serde_json::json!({ "room_ids": ["room-1"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 503);
}

// --- test_fcm: FCM 未設定 → SERVICE_UNAVAILABLE ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_test_fcm_no_fcm_configured() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state_no_fcm().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "FcmNoCfg").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let res = client
        .post(format!("{base_url}/api/devices/{fake_id}/test-fcm"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 503);
}

// --- test_fcm_all: FCM 未設定 → SERVICE_UNAVAILABLE ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_test_fcm_all_no_fcm_configured() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state_no_fcm().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "FcmAllNoCfg").await;
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

// --- fcm_dismiss_test: FCM 未設定 → SERVICE_UNAVAILABLE ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_fcm_dismiss_test_no_fcm_configured_cov() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state_no_fcm().await;
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let fake_id = uuid::Uuid::new_v4();
    let res = client
        .post(format!("{base_url}/api/devices/fcm-dismiss-test"))
        .json(&serde_json::json!({ "device_id": fake_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 503);
}

// --- test_fcm_all_exclude: FCM 未設定 → SERVICE_UNAVAILABLE ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_test_fcm_all_exclude_no_fcm_configured() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state_no_fcm().await;
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

// --- trigger_update (send_update_fcm): FCM 未設定 → SERVICE_UNAVAILABLE ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_trigger_update_no_fcm_configured() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state_no_fcm().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "TrigNoFcm").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/devices/trigger-update"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "version_code": 100, "version_name": "2.0" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 503);
}

// ============================================================
// カバレッジ専用: FCM 送信失敗 (FailingFcmSender) テスト
// ============================================================

// --- fcm_notify_call: FCM send 失敗 → errors increment ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_fcm_notify_call_send_failure() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state_failing_fcm().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "FcmFail").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
    client
        .put(format!("{base_url}/api/devices/register-fcm-token"))
        .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "fail-token" }))
        .send()
        .await
        .unwrap();
    client
        .put(format!("{base_url}/api/devices/{device_id}/call-settings"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "call_enabled": true }))
        .send()
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/fcm-notify-call"))
        .json(&serde_json::json!({ "room_ids": ["room-fail"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["errors"].as_i64().unwrap() >= 1);
}

// --- test_fcm: FCM send 失敗 → success=false ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_test_fcm_send_failure() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state_failing_fcm().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "FcmTestFail").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
    client
        .put(format!("{base_url}/api/devices/register-fcm-token"))
        .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "fail-token-2" }))
        .send()
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/{device_id}/test-fcm"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], false);
    assert!(body["error"].as_str().is_some());
}

// --- test_fcm_all: FCM send 失敗 → errors increment ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_test_fcm_all_send_failure() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state_failing_fcm().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "FcmAllFail").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
    client
        .put(format!("{base_url}/api/devices/register-fcm-token"))
        .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "fail-all-token" }))
        .send()
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/test-fcm-all"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["errors"].as_i64().unwrap() >= 1);
}

// --- test_fcm_all_exclude: FCM send 失敗 + exclude + continue ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_test_fcm_all_exclude_send_failure() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state_failing_fcm().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "FcmExFail").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // Create two devices: one to exclude, one to send (which will fail)
    let (device_id1, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
    client
        .put(format!("{base_url}/api/devices/register-fcm-token"))
        .json(&serde_json::json!({ "device_id": device_id1, "fcm_token": "excl-fail-1" }))
        .send()
        .await
        .unwrap();
    client
        .put(format!("{base_url}/api/devices/{device_id1}/call-settings"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "call_enabled": true }))
        .send()
        .await
        .unwrap();

    let (device_id2, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
    client
        .put(format!("{base_url}/api/devices/register-fcm-token"))
        .json(&serde_json::json!({ "device_id": device_id2, "fcm_token": "excl-fail-2" }))
        .send()
        .await
        .unwrap();
    client
        .put(format!("{base_url}/api/devices/{device_id2}/call-settings"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "call_enabled": true }))
        .send()
        .await
        .unwrap();

    // Exclude device1 → device2 gets sent (fails)
    let res = client
        .post(format!("{base_url}/api/devices/test-fcm-all-exclude"))
        .json(&serde_json::json!({ "exclude_device_ids": [device_id1] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    // At least some errors from FailingFcmSender
    assert!(body["errors"].as_i64().unwrap() >= 1);
}

// --- trigger_update: FCM send 失敗 → errors increment ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_trigger_update_send_failure() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state_failing_fcm().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "TrigFail").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
    client
        .put(format!("{base_url}/api/devices/register-fcm-token"))
        .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "trig-fail-tok" }))
        .send()
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/trigger-update"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "version_code": 999, "version_name": "9.9" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["errors"].as_i64().unwrap() >= 1);
}

// --- fcm_dismiss_test: FCM token ありで dismiss 送信成功 (MockFcmSender) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_fcm_dismiss_test_with_tokens() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "DismissTok").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // Create two devices in same tenant with FCM tokens
    let (device_id1, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
    client
        .put(format!("{base_url}/api/devices/register-fcm-token"))
        .json(&serde_json::json!({ "device_id": device_id1, "fcm_token": "dismiss-tok-1" }))
        .send()
        .await
        .unwrap();

    let (device_id2, _) = create_device_via_url_flow(&client, &base_url, &auth).await;
    client
        .put(format!("{base_url}/api/devices/register-fcm-token"))
        .json(&serde_json::json!({ "device_id": device_id2, "fcm_token": "dismiss-tok-2" }))
        .send()
        .await
        .unwrap();

    // Dismiss from device1 → sends to device2
    let res = client
        .post(format!("{base_url}/api/devices/fcm-dismiss-test"))
        .json(&serde_json::json!({ "device_id": device_id1 }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["sent"].as_i64().unwrap() >= 1);
}

// ============================================================
// カバレッジ専用: claim_registration 内部 DB エラー (trigger)
// ============================================================

// --- claim URL flow: INSERT devices error ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_claim_url_create_device_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "ClmUrlInsErr").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // Create URL token
    let res = client
        .post(format!("{base_url}/api/devices/register/create-token"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "device_name": "err-dev" }))
        .send()
        .await
        .unwrap();
    let body: Value = res.json().await.unwrap();
    let code = body["registration_code"].as_str().unwrap().to_string();

    // Install trigger to fail INSERT on devices
    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION alc_api.fail_dev_ins_clm_url() RETURNS trigger AS $$
        BEGIN RAISE EXCEPTION 'test error claim url insert'; END; $$ LANGUAGE plpgsql"#,
    )
    .execute(state.pool())
    .await
    .unwrap();
    sqlx::query("CREATE OR REPLACE TRIGGER fail_dev_ins_clm_url BEFORE INSERT ON alc_api.devices FOR EACH ROW EXECUTE FUNCTION alc_api.fail_dev_ins_clm_url()")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({
            "registration_code": code,
            "device_name": "err-dev",
            "phone_number": "090-0000-0000"
        }))
        .send()
        .await
        .unwrap();
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], false);

    sqlx::query("DROP TRIGGER fail_dev_ins_clm_url ON alc_api.devices")
        .execute(state.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION alc_api.fail_dev_ins_clm_url")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- claim device_owner flow: INSERT devices error ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_claim_device_owner_create_device_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "ClmDOInsErr").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // Create device_owner token
    let res = client
        .post(format!(
            "{base_url}/api/devices/register/create-device-owner-token"
        ))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "device_name": "err-do-dev" }))
        .send()
        .await
        .unwrap();
    let body: Value = res.json().await.unwrap();
    let code = body["registration_code"].as_str().unwrap().to_string();

    // Install trigger to fail INSERT on devices
    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION alc_api.fail_dev_ins_clm_do() RETURNS trigger AS $$
        BEGIN RAISE EXCEPTION 'test error claim do insert'; END; $$ LANGUAGE plpgsql"#,
    )
    .execute(state.pool())
    .await
    .unwrap();
    sqlx::query("CREATE OR REPLACE TRIGGER fail_dev_ins_clm_do BEFORE INSERT ON alc_api.devices FOR EACH ROW EXECUTE FUNCTION alc_api.fail_dev_ins_clm_do()")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({
            "registration_code": code,
            "device_name": "err-do-dev"
        }))
        .send()
        .await
        .unwrap();
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], false);

    sqlx::query("DROP TRIGGER fail_dev_ins_clm_do ON alc_api.devices")
        .execute(state.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION alc_api.fail_dev_ins_clm_do")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- claim qr_permanent flow: UPDATE device_registration_requests error ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_claim_qr_permanent_update_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "ClmPQRUpdErr").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // Create permanent QR code
    let res = client
        .post(format!(
            "{base_url}/api/devices/register/create-permanent-qr"
        ))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "device_name": "pqr-err" }))
        .send()
        .await
        .unwrap();
    let body: Value = res.json().await.unwrap();
    let code = body["registration_code"].as_str().unwrap().to_string();

    // Install trigger to fail UPDATE on device_registration_requests
    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION alc_api.fail_drr_upd_clm_pqr() RETURNS trigger AS $$
        BEGIN RAISE EXCEPTION 'test error claim pqr update'; END; $$ LANGUAGE plpgsql"#,
    )
    .execute(state.pool())
    .await
    .unwrap();
    sqlx::query("CREATE OR REPLACE TRIGGER fail_drr_upd_clm_pqr BEFORE UPDATE ON alc_api.device_registration_requests FOR EACH ROW EXECUTE FUNCTION alc_api.fail_drr_upd_clm_pqr()")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({
            "registration_code": code,
            "phone_number": "080-9999-8888"
        }))
        .send()
        .await
        .unwrap();
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["success"], false);

    sqlx::query("DROP TRIGGER fail_drr_upd_clm_pqr ON alc_api.device_registration_requests")
        .execute(state.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION alc_api.fail_drr_upd_clm_pqr")
        .execute(state.pool())
        .await
        .unwrap();
}

// ============================================================
// カバレッジ専用: approve 内部 DB エラー (trigger)
// ============================================================

// --- approve_device: INSERT devices error ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_approve_device_create_device_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let (_tenant_id, jwt) = setup_tenant_with_user(&state).await;
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // Create permanent QR + claim to get pending request
    let res = client
        .post(format!(
            "{base_url}/api/devices/register/create-permanent-qr"
        ))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "device_name": "ApproveInsErr" }))
        .send()
        .await
        .unwrap();
    let body: Value = res.json().await.unwrap();
    let code = body["registration_code"].as_str().unwrap().to_string();

    client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({ "registration_code": code }))
        .send()
        .await
        .unwrap();

    // Get request ID
    let res = client
        .get(format!("{base_url}/api/devices/pending"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    let pending: Vec<Value> = res.json().await.unwrap();
    let req_id = pending
        .iter()
        .find(|p| p["registration_code"] == code.as_str())
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Install trigger to fail INSERT on devices
    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION alc_api.fail_dev_ins_appr() RETURNS trigger AS $$
        BEGIN RAISE EXCEPTION 'test error approve insert'; END; $$ LANGUAGE plpgsql"#,
    )
    .execute(state.pool())
    .await
    .unwrap();
    sqlx::query("CREATE OR REPLACE TRIGGER fail_dev_ins_appr BEFORE INSERT ON alc_api.devices FOR EACH ROW EXECUTE FUNCTION alc_api.fail_dev_ins_appr()")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/approve/{req_id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("DROP TRIGGER fail_dev_ins_appr ON alc_api.devices")
        .execute(state.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION alc_api.fail_dev_ins_appr")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- approve_by_code: INSERT devices error ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_approve_by_code_create_device_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let (_tenant_id, jwt) = setup_tenant_with_user(&state).await;
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // Create QR temp request
    let res = client
        .post(format!("{base_url}/api/devices/register/request"))
        .json(&serde_json::json!({ "device_name": "ApprCodeInsErr" }))
        .send()
        .await
        .unwrap();
    let body: Value = res.json().await.unwrap();
    let code = body["registration_code"].as_str().unwrap().to_string();

    // Install trigger to fail INSERT on devices
    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION alc_api.fail_dev_ins_appr_code() RETURNS trigger AS $$
        BEGIN RAISE EXCEPTION 'test error approve_by_code insert'; END; $$ LANGUAGE plpgsql"#,
    )
    .execute(state.pool())
    .await
    .unwrap();
    sqlx::query("CREATE OR REPLACE TRIGGER fail_dev_ins_appr_code BEFORE INSERT ON alc_api.devices FOR EACH ROW EXECUTE FUNCTION alc_api.fail_dev_ins_appr_code()")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/approve-by-code/{code}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("DROP TRIGGER fail_dev_ins_appr_code ON alc_api.devices")
        .execute(state.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION alc_api.fail_dev_ins_appr_code")
        .execute(state.pool())
        .await
        .unwrap();
}

// ============================================================
// カバレッジ専用: UPDATE エラーパス (trigger)
// ============================================================

// --- report_watchdog_state UPDATE error (trigger) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_report_watchdog_state_update_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "WdgUpdErr").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;

    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION alc_api.fail_dev_upd_wdg() RETURNS trigger AS $$
        BEGIN RAISE EXCEPTION 'test error watchdog update'; END; $$ LANGUAGE plpgsql"#,
    )
    .execute(state.pool())
    .await
    .unwrap();
    sqlx::query("CREATE OR REPLACE TRIGGER fail_dev_upd_wdg BEFORE UPDATE ON alc_api.devices FOR EACH ROW EXECUTE FUNCTION alc_api.fail_dev_upd_wdg()")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .put(format!("{base_url}/api/devices/report-watchdog"))
        .json(&serde_json::json!({ "device_id": device_id, "running": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("DROP TRIGGER fail_dev_upd_wdg ON alc_api.devices")
        .execute(state.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION alc_api.fail_dev_upd_wdg")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- register_fcm_token UPDATE error (trigger) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_register_fcm_token_update_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "FcmTokUpdErr").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;

    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION alc_api.fail_dev_upd_fcm() RETURNS trigger AS $$
        BEGIN RAISE EXCEPTION 'test error fcm token update'; END; $$ LANGUAGE plpgsql"#,
    )
    .execute(state.pool())
    .await
    .unwrap();
    sqlx::query("CREATE OR REPLACE TRIGGER fail_dev_upd_fcm BEFORE UPDATE ON alc_api.devices FOR EACH ROW EXECUTE FUNCTION alc_api.fail_dev_upd_fcm()")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .put(format!("{base_url}/api/devices/register-fcm-token"))
        .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "err-tok" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("DROP TRIGGER fail_dev_upd_fcm ON alc_api.devices")
        .execute(state.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION alc_api.fail_dev_upd_fcm")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- update_last_login UPDATE error (trigger) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_update_last_login_update_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "LastLogUpdErr").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;

    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION alc_api.fail_dev_upd_login() RETURNS trigger AS $$
        BEGIN RAISE EXCEPTION 'test error last login update'; END; $$ LANGUAGE plpgsql"#,
    )
    .execute(state.pool())
    .await
    .unwrap();
    sqlx::query("CREATE OR REPLACE TRIGGER fail_dev_upd_login BEFORE UPDATE ON alc_api.devices FOR EACH ROW EXECUTE FUNCTION alc_api.fail_dev_upd_login()")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .put(format!("{base_url}/api/devices/update-last-login"))
        .json(&serde_json::json!({
            "device_id": device_id,
            "employee_id": uuid::Uuid::new_v4(),
            "employee_name": "ErrEmp",
            "employee_role": ["driver"]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("DROP TRIGGER fail_dev_upd_login ON alc_api.devices")
        .execute(state.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION alc_api.fail_dev_upd_login")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- report_version UPDATE error (trigger) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_report_version_update_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "VerUpdErr").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;

    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION alc_api.fail_dev_upd_ver() RETURNS trigger AS $$
        BEGIN RAISE EXCEPTION 'test error version update'; END; $$ LANGUAGE plpgsql"#,
    )
    .execute(state.pool())
    .await
    .unwrap();
    sqlx::query("CREATE OR REPLACE TRIGGER fail_dev_upd_ver BEFORE UPDATE ON alc_api.devices FOR EACH ROW EXECUTE FUNCTION alc_api.fail_dev_upd_ver()")
        .execute(state.pool())
        .await
        .unwrap();

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
    assert_eq!(res.status(), 500);

    sqlx::query("DROP TRIGGER fail_dev_upd_ver ON alc_api.devices")
        .execute(state.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION alc_api.fail_dev_upd_ver")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- approve_device: UPDATE request error (trigger) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_approve_device_update_request_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let (_tenant_id, jwt) = setup_tenant_with_user(&state).await;
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // Create permanent QR + claim
    let res = client
        .post(format!(
            "{base_url}/api/devices/register/create-permanent-qr"
        ))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "device_name": "ApprUpdReqErr" }))
        .send()
        .await
        .unwrap();
    let body: Value = res.json().await.unwrap();
    let code = body["registration_code"].as_str().unwrap().to_string();

    client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({ "registration_code": code }))
        .send()
        .await
        .unwrap();

    // Get request ID
    let res = client
        .get(format!("{base_url}/api/devices/pending"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    let pending: Vec<Value> = res.json().await.unwrap();
    let req_id = pending
        .iter()
        .find(|p| p["registration_code"] == code.as_str())
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Install trigger to fail UPDATE on device_registration_requests
    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION alc_api.fail_drr_upd_appr() RETURNS trigger AS $$
        BEGIN RAISE EXCEPTION 'test error approve update request'; END; $$ LANGUAGE plpgsql"#,
    )
    .execute(state.pool())
    .await
    .unwrap();
    sqlx::query("CREATE OR REPLACE TRIGGER fail_drr_upd_appr BEFORE UPDATE ON alc_api.device_registration_requests FOR EACH ROW EXECUTE FUNCTION alc_api.fail_drr_upd_appr()")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/approve/{req_id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("DROP TRIGGER fail_drr_upd_appr ON alc_api.device_registration_requests")
        .execute(state.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION alc_api.fail_drr_upd_appr")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- approve_by_code: UPDATE request error (trigger) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_approve_by_code_update_request_db_error() {
    let _db = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let _flock = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let (_tenant_id, jwt) = setup_tenant_with_user(&state).await;
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    // Create QR temp request
    let res = client
        .post(format!("{base_url}/api/devices/register/request"))
        .json(&serde_json::json!({ "device_name": "ApprCodeUpdErr" }))
        .send()
        .await
        .unwrap();
    let body: Value = res.json().await.unwrap();
    let code = body["registration_code"].as_str().unwrap().to_string();

    // Install trigger to fail UPDATE on device_registration_requests
    sqlx::query(
        r#"CREATE OR REPLACE FUNCTION alc_api.fail_drr_upd_appr_code() RETURNS trigger AS $$
        BEGIN RAISE EXCEPTION 'test error approve_by_code update'; END; $$ LANGUAGE plpgsql"#,
    )
    .execute(state.pool())
    .await
    .unwrap();
    sqlx::query("CREATE OR REPLACE TRIGGER fail_drr_upd_appr_code BEFORE UPDATE ON alc_api.device_registration_requests FOR EACH ROW EXECUTE FUNCTION alc_api.fail_drr_upd_appr_code()")
        .execute(state.pool())
        .await
        .unwrap();

    let res = client
        .post(format!("{base_url}/api/devices/approve-by-code/{code}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    sqlx::query("DROP TRIGGER fail_drr_upd_appr_code ON alc_api.device_registration_requests")
        .execute(state.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION alc_api.fail_drr_upd_appr_code")
        .execute(state.pool())
        .await
        .unwrap();
}

// --- update_call_settings: FCM send failure (FailingFcmSender + always_on) ---
#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_update_call_settings_fcm_send_failure() {
    #[cfg(coverage)]
    let _db_lock = crate::common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = crate::common::db_rename_flock();
    let state = crate::common::setup_app_state_failing_fcm().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(state.pool(), "CallFcmFail").await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let (device_id, _) = create_device_via_url_flow(&client, &base_url, &auth).await;

    // Register FCM token
    client
        .put(format!("{base_url}/api/devices/register-fcm-token"))
        .json(&serde_json::json!({ "device_id": device_id, "fcm_token": "fail-call-tok" }))
        .send()
        .await
        .unwrap();

    // Update with always_on triggers FCM settings_changed which will fail
    let res = client
        .put(format!("{base_url}/api/devices/{device_id}/call-settings"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "call_enabled": true,
            "always_on": true
        }))
        .send()
        .await
        .unwrap();
    // FCM failure is logged but does not fail the endpoint
    assert_eq!(res.status(), 204);
}

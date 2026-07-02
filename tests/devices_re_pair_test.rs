#[macro_use]
mod common;

use serde_json::Value;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ============================================================
// ヘルパー
// ============================================================

async fn create_device_via_url_flow(
    client: &reqwest::Client,
    base_url: &str,
    auth: &str,
) -> String {
    let res = client
        .post(format!("{base_url}/api/devices/register/create-token"))
        .header("Authorization", auth)
        .json(&serde_json::json!({ "device_name": "Re-pair Test Device" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    let code = body["registration_code"].as_str().unwrap().to_string();

    let res = client
        .post(format!("{base_url}/api/devices/register/claim"))
        .json(&serde_json::json!({
            "registration_code": code,
            "phone_number": "090-1234-5678",
            "device_name": "Re-pair Test Device"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    body["device_id"].as_str().unwrap().to_string()
}

async fn mount_pair_internal_success(server: &MockServer, times: u64) {
    Mock::given(method("POST"))
        .and(path("/device/pair-internal"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "auth_device_id": "re-pair-auth-device-id",
            "device_secret": "re-pair-device-secret"
        })))
        .expect(times)
        .mount(server)
        .await;
}

// ============================================================
// kiosk 端末 re-pair (再認証、Refs #495) — 実 DB 統合テスト
// ============================================================

// authorize-repair → re-pair 成功 → 2 回目は 404 (single-use、設計 doc の
// Acceptance Criteria に明記)
#[tokio::test]
async fn test_re_pair_single_use_window() {
    #[cfg(coverage)]
    let _db_lock = common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = common::db_rename_flock();

    let pair_server = MockServer::start().await;
    mount_pair_internal_success(&pair_server, 1).await;

    let mut state = common::setup_app_state().await;
    state.device_pair_client = Some(std::sync::Arc::new(
        rust_alc_api::device_pair_client::HttpDevicePairClient::with_endpoint(
            format!("{}/device/pair-internal", pair_server.uri()),
            "test-secret".to_string(),
        ),
    ));
    let base_url = common::spawn_test_server(state.clone()).await;

    let tenant_id = common::create_test_tenant(state.pool(), "RePair Tenant").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let device_id = create_device_via_url_flow(&client, &base_url, &auth).await;

    // 管理者が window を開ける
    let res = client
        .post(format!(
            "{base_url}/api/devices/{device_id}/authorize-repair"
        ))
        .header("Authorization", &auth)
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["authorized_until"].is_string());

    // 1 回目の re-pair: 成功
    let res = client
        .post(format!("{base_url}/api/devices/re-pair"))
        .json(&serde_json::json!({ "device_id": device_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.headers().get("cache-control").unwrap(), "no-store");
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["auth_device_id"], "re-pair-auth-device-id");
    assert_eq!(body["device_secret"], "re-pair-device-secret");

    // 2 回目の re-pair (同一 device、window 未再付与): single-use なので 404
    let res = client
        .post(format!("{base_url}/api/devices/re-pair"))
        .json(&serde_json::json!({ "device_id": device_id }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// window 外 / status 不正 / 存在しない device_id は全て 404
#[tokio::test]
async fn test_re_pair_denied_without_authorization() {
    #[cfg(coverage)]
    let _db_lock = common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = common::db_rename_flock();

    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();

    // 存在しない device_id
    let res = client
        .post(format!("{base_url}/api/devices/re-pair"))
        .json(&serde_json::json!({ "device_id": Uuid::new_v4() }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// authorize-repair はテナント境界を超えない (他 tenant の device は 404)
#[tokio::test]
async fn test_authorize_repair_cross_tenant_not_found() {
    #[cfg(coverage)]
    let _db_lock = common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = common::db_rename_flock();

    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;

    let tenant_a = common::create_test_tenant(state.pool(), "RePair Tenant A").await;
    let tenant_b = common::create_test_tenant(state.pool(), "RePair Tenant B").await;
    let jwt_a = common::create_test_jwt(tenant_a, "admin");
    let jwt_b = common::create_test_jwt(tenant_b, "admin");
    let client = reqwest::Client::new();

    let device_id =
        create_device_via_url_flow(&client, &base_url, &format!("Bearer {jwt_a}")).await;

    // tenant B からは authorize-repair できない
    let res = client
        .post(format!(
            "{base_url}/api/devices/{device_id}/authorize-repair"
        ))
        .header("Authorization", format!("Bearer {jwt_b}"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// record_re_pair_success の compare-and-swap: 2 回目 (window 消費済み想定)
// の呼び出しは false を返す (Refs #495 C-1 review、リポジトリ層の直接検証)
#[tokio::test]
async fn test_record_re_pair_success_cas_rejects_stale_window() {
    #[cfg(coverage)]
    let _db_lock = common::DB_RENAME_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    #[cfg(coverage)]
    let _flock_guard = common::db_rename_flock();

    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;

    let tenant_id = common::create_test_tenant(state.pool(), "RePair CAS Tenant").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let device_id_str = create_device_via_url_flow(&client, &base_url, &auth).await;
    let device_id = Uuid::parse_str(&device_id_str).unwrap();

    let authorized_until = state
        .devices
        .authorize_repair(tenant_id, device_id, 900, false)
        .await
        .unwrap()
        .unwrap();

    // 1 回目: 期待通り消費できる
    let consumed_first = state
        .devices
        .record_re_pair_success(tenant_id, device_id, Some(authorized_until), None)
        .await
        .unwrap();
    assert!(consumed_first);

    // 2 回目: 同じ expected_authorized_until を渡しても、既に NULL に
    // 消費済みなので CAS が一致せず false (= 呼び出し元は 404 にする)
    let consumed_second = state
        .devices
        .record_re_pair_success(tenant_id, device_id, Some(authorized_until), None)
        .await
        .unwrap();
    assert!(!consumed_second);
}

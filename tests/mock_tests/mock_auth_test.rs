// 認証ルートの mock テスト。
//
// #479 PR-3 で旧ログイン経路 (Google / LINE / LINE WORKS OAuth / WOFF /
// password login / refresh / switch-org = auth::public_router + switch_org) は
// auth-worker へ完全移管され rust から撤去された。残存 endpoint
// (me / logout / my-orgs) のテストのみをここに残す。

use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde_json::Value;

use crate::mock_helpers::app_state::setup_mock_app_state;
use crate::mock_helpers::MockAuthRepository;

use rust_alc_api::db::models::Tenant;

// ============================================================
// protected route — invalid/malformed Bearer token returns 401
// ============================================================

#[tokio::test]
async fn test_require_jwt_invalid_token_returns_401() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let client = reqwest::Client::new();
    // 不正な Bearer token を保護 endpoint (auth::protected_router) に送る
    let res = client
        .get(format!("{base_url}/api/auth/me"))
        .header("Authorization", "Bearer invalid-token-here")
        .send()
        .await
        .unwrap();

    // test_proxy_inject の decode に失敗 → identity ヘッダー非注入 →
    // require_tenant_header で 401
    assert_eq!(res.status(), 401);
}

// ============================================================
// GET /api/auth/me — success
// ============================================================

#[tokio::test]
async fn test_me_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/auth/me"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["email"], "test@example.com");
    assert_eq!(body["role"], "admin");
    assert_eq!(body["tenant_id"], tenant_id.to_string());
}

// ============================================================
// GET /api/auth/me — unauthorized (no token)
// ============================================================

#[tokio::test]
async fn test_me_unauthorized() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/auth/me"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

// ============================================================
// POST /api/auth/logout — success
// ============================================================

#[tokio::test]
async fn test_logout_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/logout"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 204);
}

// ============================================================
// POST /api/auth/logout — unauthorized
// ============================================================

#[tokio::test]
async fn test_logout_unauthorized() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/logout"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

// ============================================================
// POST /api/auth/logout — DB error on clear_refresh_token
// ============================================================

#[tokio::test]
async fn test_logout_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(MockAuthRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/logout"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// ============================================================
// POST /api/my-orgs — success (tenant found)
// ============================================================

#[tokio::test]
async fn test_my_orgs_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_tenant.lock().unwrap() = Some(Tenant {
        id: tenant_id,
        name: "Test Org".to_string(),
        slug: Some("test-org".to_string()),
        email_domain: None,
        short_id: "deadbeef".to_string(),
        created_at: chrono::Utc::now(),
    });

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/my-orgs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    let orgs = body["organizations"].as_array().unwrap();
    assert_eq!(orgs.len(), 1);
    assert_eq!(orgs[0]["name"], "Test Org");
    assert_eq!(orgs[0]["slug"], "test-org");
    assert_eq!(orgs[0]["role"], "admin");
}

// ============================================================
// POST /api/my-orgs — empty (tenant not found)
// ============================================================

#[tokio::test]
async fn test_my_orgs_empty() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let tenant_id = Uuid::new_v4();
    // return_tenant = None → empty organizations
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/my-orgs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    let orgs = body["organizations"].as_array().unwrap();
    assert_eq!(orgs.len(), 0);
}

// ============================================================
// POST /api/my-orgs — DB error
// ============================================================

#[tokio::test]
async fn test_my_orgs_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(MockAuthRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/my-orgs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// ============================================================
// POST /api/my-orgs — unauthorized
// ============================================================

#[tokio::test]
async fn test_my_orgs_unauthorized() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/my-orgs"))
        .send()
        .await
        .unwrap();

    // my-orgs は protected_router (require_tenant_header) 配下
    assert_eq!(res.status(), 401);
}

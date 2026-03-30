#[macro_use]
mod common;
mod mock_helpers;

use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde_json::Value;

use mock_helpers::app_state::setup_mock_app_state;
use mock_helpers::{mock_user, MockAuthRepository};

use rust_alc_api::db::models::{Tenant, TenantAllowedEmail};
use rust_alc_api::db::repository::auth::SsoConfigRow;

// ============================================================
// require_jwt — invalid/malformed JWT returns 401
// ============================================================

#[tokio::test]
async fn test_require_jwt_invalid_token_returns_401() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    // Send a malformed JWT token to a require_jwt protected endpoint (auth::protected_router)
    let res = client
        .get(format!("{base_url}/api/auth/me"))
        .header("Authorization", "Bearer invalid-token-here")
        .send()
        .await
        .unwrap();

    // verify_access_token fails → 401
    assert_eq!(res.status(), 401);
}

// ============================================================
// POST /api/auth/google — google_login (existing user)
// ============================================================

#[tokio::test]
async fn test_google_login_existing_user() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();
    let user = mock_user(tenant_id);

    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_user.lock().unwrap() = Some(user.clone());

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/google"))
        .json(&serde_json::json!({ "id_token": "test-valid-token" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["access_token"].is_string());
    assert!(body["refresh_token"].is_string());
    assert_eq!(body["expires_in"], 3600);
    assert_eq!(body["user"]["email"], user.email);
    assert_eq!(body["user"]["name"], user.name);
    assert_eq!(body["user"]["role"], user.role);
}

// ============================================================
// POST /api/auth/google — google_login (new user, new tenant)
// ============================================================

#[tokio::test]
async fn test_google_login_new_user_new_tenant() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    // return_user = None → no invitation → no domain tenant → create new tenant + user
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/google"))
        .json(&serde_json::json!({ "id_token": "test-valid-token" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["access_token"].is_string());
    assert_eq!(body["user"]["email"], "google-test@example.com");
    assert_eq!(body["user"]["role"], "admin"); // new tenant → admin
}

// ============================================================
// POST /api/auth/google — new user via invitation
// ============================================================

#[tokio::test]
async fn test_google_login_new_user_via_invitation() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(MockAuthRepository::default());
    // No existing user, but invitation exists
    *mock.return_invitation.lock().unwrap() = Some(TenantAllowedEmail {
        id: Uuid::new_v4(),
        tenant_id,
        email: "google-test@example.com".to_string(),
        role: "viewer".to_string(),
        created_at: chrono::Utc::now(),
    });

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/google"))
        .json(&serde_json::json!({ "id_token": "test-valid-token" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["user"]["role"], "viewer"); // invitation role
}

// ============================================================
// POST /api/auth/google — new user via email domain match
// ============================================================

#[tokio::test]
async fn test_google_login_new_user_via_email_domain() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(MockAuthRepository::default());
    // No existing user, no invitation, but email domain matches
    *mock.return_domain_tenant.lock().unwrap() = Some(Tenant {
        id: tenant_id,
        name: "Domain Tenant".to_string(),
        slug: Some("domain-tenant".to_string()),
        email_domain: Some("example.com".to_string()),
        created_at: chrono::Utc::now(),
    });

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/google"))
        .json(&serde_json::json!({ "id_token": "test-valid-token" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["user"]["role"], "admin"); // domain match → admin
    assert_eq!(body["user"]["tenant_id"], tenant_id.to_string());
}

// ============================================================
// POST /api/auth/google — invalid token
// ============================================================

#[tokio::test]
async fn test_google_login_invalid_token() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/google"))
        .json(&serde_json::json!({ "id_token": "invalid-token" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

// ============================================================
// POST /api/auth/google — DB error on find_user_by_google_sub
// ============================================================

#[tokio::test]
async fn test_google_login_db_error() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/google"))
        .json(&serde_json::json!({ "id_token": "test-valid-token" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// ============================================================
// POST /api/auth/refresh — valid refresh token
// ============================================================

#[tokio::test]
async fn test_refresh_token_success() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();
    let user = mock_user(tenant_id);

    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_refresh_user.lock().unwrap() = Some(user);

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/refresh"))
        .json(&serde_json::json!({ "refresh_token": "some-refresh-token" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["access_token"].is_string());
    assert_eq!(body["expires_in"], 3600);
}

// ============================================================
// POST /api/auth/refresh — invalid/expired token (no user found)
// ============================================================

#[tokio::test]
async fn test_refresh_token_invalid() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    // return_refresh_user = None → user not found → 401
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/refresh"))
        .json(&serde_json::json!({ "refresh_token": "expired-token" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

// ============================================================
// POST /api/auth/refresh — DB error
// ============================================================

#[tokio::test]
async fn test_refresh_token_db_error() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/refresh"))
        .json(&serde_json::json!({ "refresh_token": "any-token" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// ============================================================
// GET /api/auth/me — success (JWT)
// ============================================================

#[tokio::test]
async fn test_me_success() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let jwt = common::create_test_jwt(tenant_id, "admin");
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
// GET /api/auth/me — unauthorized (no JWT)
// ============================================================

#[tokio::test]
async fn test_me_unauthorized() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(MockAuthRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_tenant.lock().unwrap() = Some(Tenant {
        id: tenant_id,
        name: "Test Org".to_string(),
        slug: Some("test-org".to_string()),
        email_domain: None,
        created_at: chrono::Utc::now(),
    });

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();
    // return_tenant = None → empty organizations
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(MockAuthRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/my-orgs"))
        .send()
        .await
        .unwrap();

    // my-orgs is under protected_router (require_jwt)
    assert_eq!(res.status(), 401);
}

// ============================================================
// POST /api/auth/tenants — create tenant success
// ============================================================

#[tokio::test]
async fn test_create_tenant_success() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/tenants"))
        .json(&serde_json::json!({ "name": "New Tenant" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 201);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["name"], "New Tenant");
    assert!(body["id"].is_string());
}

// ============================================================
// POST /api/auth/tenants — DB error
// ============================================================

#[tokio::test]
async fn test_create_tenant_db_error() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/tenants"))
        .json(&serde_json::json!({ "name": "New Tenant" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// ============================================================
// GET /api/auth/woff-config — success
// ============================================================

#[tokio::test]
async fn test_woff_config_success() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_sso_config.lock().unwrap() = Some(SsoConfigRow {
        tenant_id: Uuid::new_v4(),
        client_id: "test-client-id".to_string(),
        client_secret_encrypted: "encrypted".to_string(),
        external_org_id: "org-1".to_string(),
        woff_id: Some("woff-12345".to_string()),
    });

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!(
            "{base_url}/api/auth/woff-config?domain=test-domain"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["woffId"], "woff-12345");
}

// ============================================================
// GET /api/auth/woff-config — not found (no SSO config)
// ============================================================

#[tokio::test]
async fn test_woff_config_not_found() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    // return_sso_config = None → 404
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!(
            "{base_url}/api/auth/woff-config?domain=unknown-domain"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

// ============================================================
// GET /api/auth/woff-config — SSO config exists but no woff_id
// ============================================================

#[tokio::test]
async fn test_woff_config_no_woff_id() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_sso_config.lock().unwrap() = Some(SsoConfigRow {
        tenant_id: Uuid::new_v4(),
        client_id: "test-client-id".to_string(),
        client_secret_encrypted: "encrypted".to_string(),
        external_org_id: "org-1".to_string(),
        woff_id: None, // no woff_id configured
    });

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!(
            "{base_url}/api/auth/woff-config?domain=test-domain"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

// ============================================================
// GET /api/auth/woff-config — DB error
// ============================================================

#[tokio::test]
async fn test_woff_config_db_error() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!(
            "{base_url}/api/auth/woff-config?domain=test-domain"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// ============================================================
// GET /api/auth/woff-config — missing domain param
// ============================================================

#[tokio::test]
async fn test_woff_config_missing_domain() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/auth/woff-config"))
        .send()
        .await
        .unwrap();

    // Missing required query param → 400
    assert_eq!(res.status(), 400);
}

// ============================================================
// POST /api/auth/google/code — code exchange (test verifier)
// ============================================================

#[tokio::test]
async fn test_google_code_login_valid_code() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    // GoogleTokenVerifier in test mode accepts "test-valid-code"
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/google/code"))
        .json(&serde_json::json!({
            "code": "test-valid-code",
            "redirect_uri": "http://localhost"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["access_token"].is_string());
    assert_eq!(body["user"]["email"], "google-test@example.com");
}

// ============================================================
// POST /api/auth/google/code — invalid code
// ============================================================

#[tokio::test]
async fn test_google_code_login_invalid_code() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/google/code"))
        .json(&serde_json::json!({
            "code": "bad-code",
            "redirect_uri": "http://localhost"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

// ============================================================
// GET /api/auth/lineworks/redirect — missing domain/address → 400
// ============================================================

#[tokio::test]
async fn test_lineworks_redirect_missing_domain() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);
    std::env::set_var("OAUTH_STATE_SECRET", "test-state-secret");

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/lineworks/redirect?redirect_uri=https://example.com"
        ))
        .send()
        .await
        .unwrap();

    // Neither domain nor address → 400
    assert_eq!(res.status(), 400);
}

// ============================================================
// GET /api/auth/lineworks/redirect — SSO config not found → 404
// ============================================================

#[tokio::test]
async fn test_lineworks_redirect_sso_not_found() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);
    std::env::set_var("OAUTH_STATE_SECRET", "test-state-secret");

    // return_sso_config = None → 404
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/lineworks/redirect?domain=unknown&redirect_uri=https://example.com"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

// ============================================================
// GET /api/auth/lineworks/redirect — SSO config found → redirect
// ============================================================

#[tokio::test]
async fn test_lineworks_redirect_success() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);
    std::env::set_var("OAUTH_STATE_SECRET", "test-state-secret");

    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_sso_config.lock().unwrap() = Some(SsoConfigRow {
        tenant_id: Uuid::new_v4(),
        client_id: "lw-client-id".to_string(),
        client_secret_encrypted: "encrypted".to_string(),
        external_org_id: "lw-org".to_string(),
        woff_id: None,
    });

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/lineworks/redirect?domain=test&redirect_uri=https://example.com"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 307);
    let location = res.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.contains("auth.worksmobile.com"));
    assert!(location.contains("lw-client-id"));
}

// ============================================================
// GET /api/auth/lineworks/redirect — with address param
// ============================================================

#[tokio::test]
async fn test_lineworks_redirect_with_address() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);
    std::env::set_var("OAUTH_STATE_SECRET", "test-state-secret");

    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_sso_config.lock().unwrap() = Some(SsoConfigRow {
        tenant_id: Uuid::new_v4(),
        client_id: "lw-client-id".to_string(),
        client_secret_encrypted: "encrypted".to_string(),
        external_org_id: "lw-org".to_string(),
        woff_id: None,
    });

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    // address=user@test-domain → domain extracted as "test-domain"
    let res = client
        .get(format!(
            "{base_url}/api/auth/lineworks/redirect?address=user@test-domain&redirect_uri=https://example.com"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 307);
}

// ============================================================
// GET /api/auth/lineworks/redirect — DB error
// ============================================================

#[tokio::test]
async fn test_lineworks_redirect_db_error() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);
    std::env::set_var("OAUTH_STATE_SECRET", "test-state-secret");

    let mock = Arc::new(MockAuthRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/lineworks/redirect?domain=test&redirect_uri=https://example.com"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// ============================================================
// GET /api/auth/google/redirect — success (redirect to Google)
// ============================================================

#[tokio::test]
async fn test_google_redirect_success() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);
    std::env::set_var("OAUTH_STATE_SECRET", "test-state-secret");

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/google/redirect?redirect_uri=https://example.com/callback"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 307);
    let location = res.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.contains("accounts.google.com"));
    assert!(location.contains("test-google-client-id"));
}

// ============================================================
// GET /api/auth/google/redirect — missing OAUTH_STATE_SECRET → 500
// ============================================================

#[tokio::test]
async fn test_google_redirect_missing_state_secret() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);
    std::env::remove_var("OAUTH_STATE_SECRET");

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/google/redirect?redirect_uri=https://example.com/callback"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

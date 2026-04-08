use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde_json::Value;

use crate::mock_helpers::app_state::setup_mock_app_state;
use crate::mock_helpers::{mock_user, MockAuthRepository};

use rust_alc_api::auth::lineworks;
use rust_alc_api::db::models::{Tenant, TenantAllowedEmail, User};
use rust_alc_api::db::repository::auth::SsoConfigRow;

// ============================================================
// require_jwt — invalid/malformed JWT returns 401
// ============================================================

#[tokio::test]
async fn test_require_jwt_invalid_token_returns_401() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();
    let user = mock_user(tenant_id);

    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_user.lock().unwrap() = Some(user.clone());

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    // return_user = None → no invitation → no domain tenant → create new tenant + user
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();
    let user = mock_user(tenant_id);

    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_refresh_user.lock().unwrap() = Some(user);

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    // return_refresh_user = None → user not found → 401
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
// GET /api/auth/me — unauthorized (no JWT)
// ============================================================

#[tokio::test]
async fn test_me_unauthorized() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(MockAuthRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
    let base_url = crate::common::spawn_test_server(state).await;

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
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();
    // return_tenant = None → empty organizations
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(MockAuthRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    // return_sso_config = None → 404
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    // GoogleTokenVerifier in test mode accepts "test-valid-code"
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("OAUTH_STATE_SECRET", "test-state-secret");

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("OAUTH_STATE_SECRET", "test-state-secret");

    // return_sso_config = None → 404
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
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
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
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
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("OAUTH_STATE_SECRET", "test-state-secret");

    let mock = Arc::new(MockAuthRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("OAUTH_STATE_SECRET", "test-state-secret");

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::remove_var("OAUTH_STATE_SECRET");

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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

// ============================================================
// Helper: encrypt a client_secret for lineworks decrypt_secret
// ============================================================

fn encrypt_secret_for_test(plaintext: &str, key_material: &str) -> String {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
    use ring::rand::{SecureRandom, SystemRandom};
    use sha2::{Digest, Sha256};

    let mut key_bytes = [0u8; 32];
    let hash = Sha256::digest(key_material.as_bytes());
    key_bytes.copy_from_slice(&hash);

    let unbound_key = UnboundKey::new(&AES_256_GCM, &key_bytes).unwrap();
    let key = LessSafeKey::new(unbound_key);

    let rng = SystemRandom::new();
    let mut nonce_bytes = [0u8; 12];
    rng.fill(&mut nonce_bytes).unwrap();
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    let mut in_out = plaintext.as_bytes().to_vec();
    let tag_len = aead::AES_256_GCM.tag_len();
    in_out.extend(vec![0u8; tag_len]);

    let tag = key
        .seal_in_place_separate_tag(nonce, Aad::empty(), &mut in_out[..plaintext.len()])
        .unwrap();
    in_out[plaintext.len()..].copy_from_slice(tag.as_ref());

    let mut result = nonce_bytes.to_vec();
    result.extend_from_slice(&in_out);
    BASE64.encode(&result)
}

// ============================================================
// GET /api/auth/google/callback — success (code exchange + redirect)
// ============================================================

#[tokio::test]
async fn test_google_callback_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("OAUTH_STATE_SECRET", "test-state-secret");
    std::env::set_var("API_ORIGIN", "http://localhost:0");

    let tenant_id = Uuid::new_v4();
    let user = mock_user(tenant_id);

    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_user.lock().unwrap() = Some(user.clone());

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    // Create a valid signed state
    let state_payload = lineworks::state::StatePayload {
        redirect_uri: "https://items.mtamaramu.com/callback".to_string(),
        nonce: Uuid::new_v4().to_string(),
        provider: "google".to_string(),
        external_org_id: String::new(),
    };
    let signed_state = lineworks::state::sign(&state_payload, "test-state-secret");

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/google/callback?code=test-valid-code&state={}",
            urlencoding::encode(&signed_state)
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 307);
    let location = res.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.starts_with("https://items.mtamaramu.com/callback#token="));
    assert!(location.contains("refresh_token="));
    assert!(location.contains("lw_callback=1"));

    // Check Set-Cookie header with parent domain extraction
    let cookie = res.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(cookie.contains("logi_auth_token="));
    assert!(cookie.contains("Domain=.mtamaramu.com"));
}

// ============================================================
// GET /api/auth/google/callback — missing OAUTH_STATE_SECRET → 500
// ============================================================

#[tokio::test]
async fn test_google_callback_missing_state_secret() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::remove_var("OAUTH_STATE_SECRET");

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/google/callback?code=test-valid-code&state=invalid"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// ============================================================
// GET /api/auth/google/callback — invalid state → 400
// ============================================================

#[tokio::test]
async fn test_google_callback_invalid_state() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("OAUTH_STATE_SECRET", "test-state-secret");

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/google/callback?code=test-valid-code&state=bad-state"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

// ============================================================
// GET /api/auth/google/callback — invalid code → 502
// ============================================================

#[tokio::test]
async fn test_google_callback_invalid_code() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("OAUTH_STATE_SECRET", "test-state-secret");

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let state_payload = lineworks::state::StatePayload {
        redirect_uri: "https://example.com/callback".to_string(),
        nonce: Uuid::new_v4().to_string(),
        provider: "google".to_string(),
        external_org_id: String::new(),
    };
    let signed_state = lineworks::state::sign(&state_payload, "test-state-secret");

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/google/callback?code=bad-code&state={}",
            urlencoding::encode(&signed_state)
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 502);
}

// ============================================================
// GET /api/auth/google/callback — new user (no existing user)
// ============================================================

#[tokio::test]
async fn test_google_callback_new_user() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("OAUTH_STATE_SECRET", "test-state-secret");

    // return_user = None → new user → new tenant
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let state_payload = lineworks::state::StatePayload {
        redirect_uri: "https://sub.example.com/callback".to_string(),
        nonce: Uuid::new_v4().to_string(),
        provider: "google".to_string(),
        external_org_id: String::new(),
    };
    let signed_state = lineworks::state::sign(&state_payload, "test-state-secret");

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/google/callback?code=test-valid-code&state={}",
            urlencoding::encode(&signed_state)
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 307);
    let location = res.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.starts_with("https://sub.example.com/callback#token="));

    // extract_parent_domain: "sub.example.com" → "example.com"
    let cookie = res.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(cookie.contains("Domain=.example.com"));
}

// ============================================================
// GET /api/auth/lineworks/redirect — missing OAUTH_STATE_SECRET → 500
// ============================================================

#[tokio::test]
async fn test_lineworks_redirect_missing_state_secret() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::remove_var("OAUTH_STATE_SECRET");

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
// GET /api/auth/lineworks/callback — via wiremock (success, new user)
// ============================================================

#[tokio::test]
async fn test_lineworks_callback_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    let oauth_secret = "test-state-secret";
    std::env::set_var("OAUTH_STATE_SECRET", oauth_secret);

    // Start wiremock server
    let mock_server = wiremock::MockServer::start().await;

    // Set LINE WORKS endpoints to wiremock
    std::env::set_var(
        "LINEWORKS_TOKEN_URL",
        format!("{}/oauth2/token", mock_server.uri()),
    );
    std::env::set_var(
        "LINEWORKS_USERINFO_URL",
        format!("{}/v1.0/users/me", mock_server.uri()),
    );

    // Mock token exchange
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/oauth2/token"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "lw-access-token-123",
                "token_type": "Bearer",
                "expires_in": 3600,
                "scope": "user.profile.read"
            })),
        )
        .mount(&mock_server)
        .await;

    // Mock user profile
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v1.0/users/me"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "userId": "lw-user-001",
                "userName": {
                    "lastName": "田中",
                    "firstName": "太郎"
                },
                "email": "tanaka@example.com"
            })),
        )
        .mount(&mock_server)
        .await;

    let tenant_id = Uuid::new_v4();
    let jwt_secret = crate::common::TEST_JWT_SECRET;

    // Encrypt a client_secret for the SSO config
    let encrypted_secret = encrypt_secret_for_test("test-client-secret", jwt_secret);

    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_sso_config.lock().unwrap() = Some(SsoConfigRow {
        tenant_id,
        client_id: "lw-client-id".to_string(),
        client_secret_encrypted: encrypted_secret,
        external_org_id: "lw-org".to_string(),
        woff_id: None,
    });

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    // Create valid state (provider=lineworks, external_org_id must match SSO config)
    let state_payload = lineworks::state::StatePayload {
        redirect_uri: "https://items.mtamaramu.com/callback".to_string(),
        nonce: Uuid::new_v4().to_string(),
        provider: "lineworks".to_string(),
        external_org_id: "lw-org".to_string(),
    };
    let signed_state = lineworks::state::sign(&state_payload, oauth_secret);

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/lineworks/callback?code=lw-auth-code&state={}",
            urlencoding::encode(&signed_state)
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 307);
    let location = res.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.starts_with("https://items.mtamaramu.com/callback#token="));
    assert!(location.contains("refresh_token="));

    let cookie = res.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(cookie.contains("Domain=.mtamaramu.com"));

    // Cleanup env vars
    std::env::remove_var("LINEWORKS_TOKEN_URL");
    std::env::remove_var("LINEWORKS_USERINFO_URL");
}

// ============================================================
// GET /api/auth/lineworks/callback — existing user (upsert path)
// ============================================================

#[tokio::test]
async fn test_lineworks_callback_existing_user() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    let oauth_secret = "test-state-secret";
    std::env::set_var("OAUTH_STATE_SECRET", oauth_secret);

    let mock_server = wiremock::MockServer::start().await;
    std::env::set_var(
        "LINEWORKS_TOKEN_URL",
        format!("{}/oauth2/token", mock_server.uri()),
    );
    std::env::set_var(
        "LINEWORKS_USERINFO_URL",
        format!("{}/v1.0/users/me", mock_server.uri()),
    );

    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/oauth2/token"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "lw-access-token-123",
                "token_type": "Bearer",
                "expires_in": 3600
            })),
        )
        .mount(&mock_server)
        .await;

    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v1.0/users/me"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "userId": "lw-user-001",
                "userName": { "lastName": "田中", "firstName": "太郎" },
                "email": "tanaka@example.com"
            })),
        )
        .mount(&mock_server)
        .await;

    let tenant_id = Uuid::new_v4();
    let jwt_secret = crate::common::TEST_JWT_SECRET;
    let encrypted_secret = encrypt_secret_for_test("test-client-secret", jwt_secret);

    // Pre-set an existing lineworks user
    let existing_user = User {
        id: Uuid::new_v4(),
        tenant_id,
        google_sub: None,
        lineworks_id: Some("lw-user-001".to_string()),
        line_user_id: None,
        email: "tanaka@example.com".to_string(),
        name: "田中太郎".to_string(),
        role: "admin".to_string(),
        username: None,
        password_hash: None,
        refresh_token_hash: None,
        refresh_token_expires_at: None,
        created_at: chrono::Utc::now(),
    };

    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_sso_config.lock().unwrap() = Some(SsoConfigRow {
        tenant_id,
        client_id: "lw-client-id".to_string(),
        client_secret_encrypted: encrypted_secret,
        external_org_id: "lw-org".to_string(),
        woff_id: None,
    });
    *mock.return_lineworks_user.lock().unwrap() = Some(existing_user);

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let state_payload = lineworks::state::StatePayload {
        redirect_uri: "https://app.example.com/cb".to_string(),
        nonce: Uuid::new_v4().to_string(),
        provider: "lineworks".to_string(),
        external_org_id: "lw-org".to_string(),
    };
    let signed_state = lineworks::state::sign(&state_payload, oauth_secret);

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/lineworks/callback?code=lw-auth-code&state={}",
            urlencoding::encode(&signed_state)
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 307);

    std::env::remove_var("LINEWORKS_TOKEN_URL");
    std::env::remove_var("LINEWORKS_USERINFO_URL");
}

// ============================================================
// GET /api/auth/lineworks/callback — missing OAUTH_STATE_SECRET → 500
// ============================================================

#[tokio::test]
async fn test_lineworks_callback_missing_state_secret() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::remove_var("OAUTH_STATE_SECRET");

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/lineworks/callback?code=any&state=any"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// ============================================================
// GET /api/auth/lineworks/callback — invalid state → 400
// ============================================================

#[tokio::test]
async fn test_lineworks_callback_invalid_state() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("OAUTH_STATE_SECRET", "test-state-secret");

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/lineworks/callback?code=any&state=invalid-state"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

// ============================================================
// GET /api/auth/lineworks/callback — SSO config not found → 500
// ============================================================

#[tokio::test]
async fn test_lineworks_callback_sso_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    let oauth_secret = "test-state-secret";
    std::env::set_var("OAUTH_STATE_SECRET", oauth_secret);

    // return_sso_config = None → resolve_sso_config_required fails
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let state_payload = lineworks::state::StatePayload {
        redirect_uri: "https://example.com/callback".to_string(),
        nonce: Uuid::new_v4().to_string(),
        provider: "lineworks".to_string(),
        external_org_id: "lw-org".to_string(),
    };
    let signed_state = lineworks::state::sign(&state_payload, oauth_secret);

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/lineworks/callback?code=any&state={}",
            urlencoding::encode(&signed_state)
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// ============================================================
// GET /api/auth/lineworks/callback — decrypt_secret fails → 500
// ============================================================

#[tokio::test]
async fn test_lineworks_callback_decrypt_fails() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    let oauth_secret = "test-state-secret";
    std::env::set_var("OAUTH_STATE_SECRET", oauth_secret);

    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_sso_config.lock().unwrap() = Some(SsoConfigRow {
        tenant_id,
        client_id: "lw-client-id".to_string(),
        client_secret_encrypted: "invalid-base64-not-encrypted".to_string(),
        external_org_id: "lw-org".to_string(),
        woff_id: None,
    });

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let state_payload = lineworks::state::StatePayload {
        redirect_uri: "https://example.com/callback".to_string(),
        nonce: Uuid::new_v4().to_string(),
        provider: "lineworks".to_string(),
        external_org_id: "lw-org".to_string(),
    };
    let signed_state = lineworks::state::sign(&state_payload, oauth_secret);

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/lineworks/callback?code=any&state={}",
            urlencoding::encode(&signed_state)
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// ============================================================
// GET /api/auth/lineworks/callback — token exchange fails → 502
// ============================================================

#[tokio::test]
async fn test_lineworks_callback_token_exchange_fails() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    let oauth_secret = "test-state-secret";
    std::env::set_var("OAUTH_STATE_SECRET", oauth_secret);

    let mock_server = wiremock::MockServer::start().await;
    std::env::set_var(
        "LINEWORKS_TOKEN_URL",
        format!("{}/oauth2/token", mock_server.uri()),
    );

    // Token exchange returns 400
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/oauth2/token"))
        .respond_with(
            wiremock::ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": "invalid_grant"
            })),
        )
        .mount(&mock_server)
        .await;

    let tenant_id = Uuid::new_v4();
    let jwt_secret = crate::common::TEST_JWT_SECRET;
    let encrypted_secret = encrypt_secret_for_test("test-client-secret", jwt_secret);

    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_sso_config.lock().unwrap() = Some(SsoConfigRow {
        tenant_id,
        client_id: "lw-client-id".to_string(),
        client_secret_encrypted: encrypted_secret,
        external_org_id: "lw-org".to_string(),
        woff_id: None,
    });

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let state_payload = lineworks::state::StatePayload {
        redirect_uri: "https://example.com/callback".to_string(),
        nonce: Uuid::new_v4().to_string(),
        provider: "lineworks".to_string(),
        external_org_id: "lw-org".to_string(),
    };
    let signed_state = lineworks::state::sign(&state_payload, oauth_secret);

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/lineworks/callback?code=bad-code&state={}",
            urlencoding::encode(&signed_state)
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 502);

    std::env::remove_var("LINEWORKS_TOKEN_URL");
}

// ============================================================
// GET /api/auth/lineworks/callback — profile fetch fails → 502
// ============================================================

#[tokio::test]
async fn test_lineworks_callback_profile_fetch_fails() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    let oauth_secret = "test-state-secret";
    std::env::set_var("OAUTH_STATE_SECRET", oauth_secret);

    let mock_server = wiremock::MockServer::start().await;
    std::env::set_var(
        "LINEWORKS_TOKEN_URL",
        format!("{}/oauth2/token", mock_server.uri()),
    );
    std::env::set_var(
        "LINEWORKS_USERINFO_URL",
        format!("{}/v1.0/users/me", mock_server.uri()),
    );

    // Token exchange succeeds
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/oauth2/token"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "lw-access-token-123",
                "token_type": "Bearer",
                "expires_in": 3600
            })),
        )
        .mount(&mock_server)
        .await;

    // Profile fetch fails
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v1.0/users/me"))
        .respond_with(
            wiremock::ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": "unauthorized"
            })),
        )
        .mount(&mock_server)
        .await;

    let tenant_id = Uuid::new_v4();
    let jwt_secret = crate::common::TEST_JWT_SECRET;
    let encrypted_secret = encrypt_secret_for_test("test-client-secret", jwt_secret);

    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_sso_config.lock().unwrap() = Some(SsoConfigRow {
        tenant_id,
        client_id: "lw-client-id".to_string(),
        client_secret_encrypted: encrypted_secret,
        external_org_id: "lw-org".to_string(),
        woff_id: None,
    });

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let state_payload = lineworks::state::StatePayload {
        redirect_uri: "https://example.com/callback".to_string(),
        nonce: Uuid::new_v4().to_string(),
        provider: "lineworks".to_string(),
        external_org_id: "lw-org".to_string(),
    };
    let signed_state = lineworks::state::sign(&state_payload, oauth_secret);

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/lineworks/callback?code=lw-code&state={}",
            urlencoding::encode(&signed_state)
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 502);

    std::env::remove_var("LINEWORKS_TOKEN_URL");
    std::env::remove_var("LINEWORKS_USERINFO_URL");
}

// ============================================================
// GET /api/auth/lineworks/callback — create_user_lineworks fails → 500
// ============================================================

#[tokio::test]
async fn test_lineworks_callback_create_user_fails() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    let oauth_secret = "test-state-secret";
    std::env::set_var("OAUTH_STATE_SECRET", oauth_secret);

    let mock_server = wiremock::MockServer::start().await;
    std::env::set_var(
        "LINEWORKS_TOKEN_URL",
        format!("{}/oauth2/token", mock_server.uri()),
    );
    std::env::set_var(
        "LINEWORKS_USERINFO_URL",
        format!("{}/v1.0/users/me", mock_server.uri()),
    );

    // Mock token exchange
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/oauth2/token"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "lw-access-token-123",
                "token_type": "Bearer",
                "expires_in": 3600,
                "scope": "user.profile.read"
            })),
        )
        .mount(&mock_server)
        .await;

    // Mock user profile
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v1.0/users/me"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "userId": "lw-user-new",
                "userName": {
                    "lastName": "佐藤",
                    "firstName": "花子"
                },
                "email": "sato@example.com"
            })),
        )
        .mount(&mock_server)
        .await;

    let tenant_id = Uuid::new_v4();
    let jwt_secret = crate::common::TEST_JWT_SECRET;
    let encrypted_secret = encrypt_secret_for_test("test-client-secret", jwt_secret);

    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_sso_config.lock().unwrap() = Some(SsoConfigRow {
        tenant_id,
        client_id: "lw-client-id".to_string(),
        client_secret_encrypted: encrypted_secret,
        external_org_id: "lw-org".to_string(),
        woff_id: None,
    });
    // return_lineworks_user = None → user not found → triggers create_user_lineworks
    // fail_on_create_user = true → create_user_lineworks returns Err
    mock.fail_on_create_user.store(true, Ordering::SeqCst);

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let state_payload = lineworks::state::StatePayload {
        redirect_uri: "https://items.mtamaramu.com/callback".to_string(),
        nonce: Uuid::new_v4().to_string(),
        provider: "lineworks".to_string(),
        external_org_id: "lw-org".to_string(),
    };
    let signed_state = lineworks::state::sign(&state_payload, oauth_secret);

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/lineworks/callback?code=lw-auth-code&state={}",
            urlencoding::encode(&signed_state)
        ))
        .send()
        .await
        .unwrap();

    // upsert_lineworks_user → find=None → create fails → 500
    assert_eq!(res.status(), 500);

    std::env::remove_var("LINEWORKS_TOKEN_URL");
    std::env::remove_var("LINEWORKS_USERINFO_URL");
}

// ============================================================
// POST /api/auth/woff — success (new user)
// ============================================================

#[tokio::test]
async fn test_woff_auth_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock_server = wiremock::MockServer::start().await;
    std::env::set_var(
        "LINEWORKS_USERINFO_URL",
        format!("{}/v1.0/users/me", mock_server.uri()),
    );

    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v1.0/users/me"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "userId": "woff-user-001",
                "userName": { "lastName": "佐藤", "firstName": "花子" },
                "email": "sato@example.com"
            })),
        )
        .mount(&mock_server)
        .await;

    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_sso_config.lock().unwrap() = Some(SsoConfigRow {
        tenant_id,
        client_id: "woff-client-id".to_string(),
        client_secret_encrypted: "encrypted".to_string(),
        external_org_id: "woff-org".to_string(),
        woff_id: Some("woff-12345".to_string()),
    });

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/woff"))
        .json(&serde_json::json!({
            "access_token": "woff-access-token",
            "domain_id": "woff-org"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["token"].is_string());
    assert!(body["expiresAt"].is_string());
    assert_eq!(body["tenantId"], tenant_id.to_string());

    std::env::remove_var("LINEWORKS_USERINFO_URL");
}

// ============================================================
// POST /api/auth/woff — existing user
// ============================================================

#[tokio::test]
async fn test_woff_auth_existing_user() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock_server = wiremock::MockServer::start().await;
    std::env::set_var(
        "LINEWORKS_USERINFO_URL",
        format!("{}/v1.0/users/me", mock_server.uri()),
    );

    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v1.0/users/me"))
        .respond_with(
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "userId": "woff-user-001",
                "userName": { "lastName": "佐藤", "firstName": "花子" },
                "email": "sato@example.com"
            })),
        )
        .mount(&mock_server)
        .await;

    let tenant_id = Uuid::new_v4();
    let existing_user = User {
        id: Uuid::new_v4(),
        tenant_id,
        google_sub: None,
        lineworks_id: Some("woff-user-001".to_string()),
        line_user_id: None,
        email: "sato@example.com".to_string(),
        name: "佐藤花子".to_string(),
        role: "viewer".to_string(),
        username: None,
        password_hash: None,
        refresh_token_hash: None,
        refresh_token_expires_at: None,
        created_at: chrono::Utc::now(),
    };

    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_sso_config.lock().unwrap() = Some(SsoConfigRow {
        tenant_id,
        client_id: "woff-client-id".to_string(),
        client_secret_encrypted: "encrypted".to_string(),
        external_org_id: "woff-org".to_string(),
        woff_id: Some("woff-12345".to_string()),
    });
    *mock.return_lineworks_user.lock().unwrap() = Some(existing_user);

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/woff"))
        .json(&serde_json::json!({
            "access_token": "woff-access-token",
            "domain_id": "woff-org"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);

    std::env::remove_var("LINEWORKS_USERINFO_URL");
}

// ============================================================
// POST /api/auth/woff — SSO config not found → 404
// ============================================================

#[tokio::test]
async fn test_woff_auth_sso_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    // return_sso_config = None → 404
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/woff"))
        .json(&serde_json::json!({
            "access_token": "any-token",
            "domain_id": "unknown-domain"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

// ============================================================
// POST /api/auth/woff — profile fetch fails → 401
// ============================================================

#[tokio::test]
async fn test_woff_auth_profile_fails() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock_server = wiremock::MockServer::start().await;
    std::env::set_var(
        "LINEWORKS_USERINFO_URL",
        format!("{}/v1.0/users/me", mock_server.uri()),
    );

    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v1.0/users/me"))
        .respond_with(
            wiremock::ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": "invalid_token"
            })),
        )
        .mount(&mock_server)
        .await;

    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_sso_config.lock().unwrap() = Some(SsoConfigRow {
        tenant_id,
        client_id: "woff-client-id".to_string(),
        client_secret_encrypted: "encrypted".to_string(),
        external_org_id: "woff-org".to_string(),
        woff_id: Some("woff-12345".to_string()),
    });

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/woff"))
        .json(&serde_json::json!({
            "access_token": "bad-token",
            "domain_id": "woff-org"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);

    std::env::remove_var("LINEWORKS_USERINFO_URL");
}

// ============================================================
// POST /api/auth/woff — DB error on resolve_sso_config
// ============================================================

#[tokio::test]
async fn test_woff_auth_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/woff"))
        .json(&serde_json::json!({
            "access_token": "any-token",
            "domain_id": "any-domain"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// ============================================================
// GET /api/auth/google/callback — extract_parent_domain edge cases
// ============================================================

#[tokio::test]
async fn test_google_callback_two_part_domain() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("OAUTH_STATE_SECRET", "test-state-secret");

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    // Two-part domain: "example.com" → "example.com" (no parent extraction)
    let state_payload = lineworks::state::StatePayload {
        redirect_uri: "https://example.com/callback".to_string(),
        nonce: Uuid::new_v4().to_string(),
        provider: "google".to_string(),
        external_org_id: String::new(),
    };
    let signed_state = lineworks::state::sign(&state_payload, "test-state-secret");

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/google/callback?code=test-valid-code&state={}",
            urlencoding::encode(&signed_state)
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 307);
    let cookie = res.headers().get("set-cookie").unwrap().to_str().unwrap();
    // Two-part domain stays as-is
    assert!(cookie.contains("Domain=.example.com"));
}

// ============================================================
// GET /api/auth/google/callback — redirect_uri with http (not https)
// ============================================================

#[tokio::test]
async fn test_google_callback_http_redirect_uri() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("OAUTH_STATE_SECRET", "test-state-secret");

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let state_payload = lineworks::state::StatePayload {
        redirect_uri: "http://localhost:3000/callback".to_string(),
        nonce: Uuid::new_v4().to_string(),
        provider: "google".to_string(),
        external_org_id: String::new(),
    };
    let signed_state = lineworks::state::sign(&state_payload, "test-state-secret");

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/google/callback?code=test-valid-code&state={}",
            urlencoding::encode(&signed_state)
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 307);
    let location = res.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.starts_with("http://localhost:3000/callback#token="));

    // localhost:3000 → host=localhost (port stripped), parts=1 → "localhost"
    let cookie = res.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(cookie.contains("Domain=.localhost"));
}

// ============================================================
// POST /api/auth/switch-org — success
// ============================================================

#[tokio::test]
async fn test_switch_org_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let current_tenant_id = Uuid::new_v4();
    let target_tenant_id = Uuid::new_v4();

    // The user in the target tenant
    let target_user = User {
        id: Uuid::new_v4(),
        tenant_id: target_tenant_id,
        google_sub: Some("test-google-sub".to_string()),
        lineworks_id: None,
        line_user_id: None,
        email: "test@example.com".to_string(),
        name: "Test User".to_string(),
        role: "admin".to_string(),
        username: None,
        password_hash: None,
        refresh_token_hash: None,
        refresh_token_expires_at: None,
        created_at: chrono::Utc::now(),
    };

    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_switch_user.lock().unwrap() = Some(target_user);
    *mock.return_slug.lock().unwrap() = Some("target-org".to_string());

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let jwt = crate::common::create_test_jwt(current_tenant_id, "admin");
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/switch-org"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "organization_id": target_tenant_id.to_string() }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["token"].is_string());
    assert!(body["expires_at"].is_string());
    assert_eq!(body["organization_id"], target_tenant_id.to_string());
}

// ============================================================
// POST /api/auth/switch-org — user not found in target tenant → 403
// ============================================================

#[tokio::test]
async fn test_switch_org_user_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let current_tenant_id = Uuid::new_v4();
    let target_tenant_id = Uuid::new_v4();

    // return_switch_user = None → user not found → 403
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let jwt = crate::common::create_test_jwt(current_tenant_id, "admin");
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/switch-org"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "organization_id": target_tenant_id.to_string() }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 403);
}

// ============================================================
// POST /api/auth/switch-org — invalid UUID → 400
// ============================================================

#[tokio::test]
async fn test_switch_org_invalid_uuid() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/switch-org"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "organization_id": "not-a-uuid" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

// ============================================================
// POST /api/auth/switch-org — DB error on find_user_in_tenant
// ============================================================

#[tokio::test]
async fn test_switch_org_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();
    let target_tenant_id = Uuid::new_v4();

    let mock = Arc::new(MockAuthRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/switch-org"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "organization_id": target_tenant_id.to_string() }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// ============================================================
// POST /api/auth/switch-org — unauthorized (no JWT)
// ============================================================

#[tokio::test]
async fn test_switch_org_unauthorized() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/switch-org"))
        .json(&serde_json::json!({ "organization_id": Uuid::new_v4().to_string() }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

// ============================================================
// GET /api/auth/line/redirect — success (redirect to LINE)
// ============================================================

#[tokio::test]
async fn test_line_redirect_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("OAUTH_STATE_SECRET", "test-state-secret");
    std::env::set_var("LINE_LOGIN_CHANNEL_ID", "test-line-channel-id");
    std::env::set_var("API_ORIGIN", "http://localhost:0");

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/line/redirect?redirect_uri=https://example.com/callback"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 307);
    let location = res.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.contains("access.line.me"));
    assert!(location.contains("test-line-channel-id"));

    std::env::remove_var("LINE_LOGIN_CHANNEL_ID");
}

// ============================================================
// GET /api/auth/line/redirect — with tenant_id param
// ============================================================

#[tokio::test]
async fn test_line_redirect_with_tenant_id() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("OAUTH_STATE_SECRET", "test-state-secret");
    std::env::set_var("LINE_LOGIN_CHANNEL_ID", "test-line-channel-id");
    std::env::set_var("API_ORIGIN", "http://localhost:0");

    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/line/redirect?redirect_uri=https://example.com/callback&tenant_id={}",
            tenant_id
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 307);
    let location = res.headers().get("location").unwrap().to_str().unwrap();
    assert!(location.contains("access.line.me"));

    std::env::remove_var("LINE_LOGIN_CHANNEL_ID");
}

// ============================================================
// GET /api/auth/line/redirect — missing OAUTH_STATE_SECRET → 500
// ============================================================

#[tokio::test]
async fn test_line_redirect_missing_state_secret() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::remove_var("OAUTH_STATE_SECRET");
    std::env::set_var("LINE_LOGIN_CHANNEL_ID", "test-line-channel-id");

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/line/redirect?redirect_uri=https://example.com/callback"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);

    std::env::remove_var("LINE_LOGIN_CHANNEL_ID");
}

// ============================================================
// GET /api/auth/line/redirect — missing LINE_LOGIN_CHANNEL_ID → 500
// ============================================================

#[tokio::test]
async fn test_line_redirect_missing_channel_id() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("OAUTH_STATE_SECRET", "test-state-secret");
    std::env::remove_var("LINE_LOGIN_CHANNEL_ID");

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/line/redirect?redirect_uri=https://example.com/callback"
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// ============================================================
// POST /api/auth/line/select-tenant — success (existing user)
// ============================================================

#[tokio::test]
async fn test_line_select_tenant_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();
    let user = User {
        id: Uuid::new_v4(),
        tenant_id,
        google_sub: None,
        lineworks_id: None,
        line_user_id: Some("line-user-001".to_string()),
        email: "line-user-001".to_string(),
        name: "LINE User".to_string(),
        role: "viewer".to_string(),
        username: None,
        password_hash: None,
        refresh_token_hash: None,
        refresh_token_expires_at: None,
        created_at: chrono::Utc::now(),
    };

    let mock = Arc::new(MockAuthRepository::default());
    // Set up recipients: tenant_id matches the request
    *mock.return_line_recipients.lock().unwrap() = vec![(tenant_id, "Test Tenant".to_string())];
    // Existing user found
    *mock.return_line_user.lock().unwrap() = Some(user.clone());

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/line/select-tenant"))
        .json(&serde_json::json!({
            "line_user_id": "line-user-001",
            "line_name": "LINE User",
            "tenant_id": tenant_id.to_string()
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["access_token"].is_string());
    assert!(body["refresh_token"].is_string());
    assert_eq!(body["expires_in"], 3600);
}

// ============================================================
// POST /api/auth/line/select-tenant — new user (no existing)
// ============================================================

#[tokio::test]
async fn test_line_select_tenant_new_user() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();

    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_line_recipients.lock().unwrap() = vec![(tenant_id, "Test Tenant".to_string())];
    // return_line_user = None → new user created via create_user_line

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/line/select-tenant"))
        .json(&serde_json::json!({
            "line_user_id": "line-new-user",
            "line_name": "New LINE User",
            "tenant_id": tenant_id.to_string()
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["access_token"].is_string());
    assert!(body["refresh_token"].is_string());
}

// ============================================================
// POST /api/auth/line/select-tenant — tenant not in recipients → 403
// ============================================================

#[tokio::test]
async fn test_line_select_tenant_forbidden() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();
    let other_tenant_id = Uuid::new_v4();

    let mock = Arc::new(MockAuthRepository::default());
    // Recipients list has a different tenant_id
    *mock.return_line_recipients.lock().unwrap() =
        vec![(other_tenant_id, "Other Tenant".to_string())];

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/line/select-tenant"))
        .json(&serde_json::json!({
            "line_user_id": "line-user-001",
            "line_name": "LINE User",
            "tenant_id": tenant_id.to_string()
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 403);
}

// ============================================================
// POST /api/auth/line/select-tenant — DB error on find_recipients
// ============================================================

#[tokio::test]
async fn test_line_select_tenant_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();

    let mock = Arc::new(MockAuthRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/line/select-tenant"))
        .json(&serde_json::json!({
            "line_user_id": "line-user-001",
            "line_name": "LINE User",
            "tenant_id": tenant_id.to_string()
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// ============================================================
// POST /api/auth/login — password login success
// ============================================================

#[tokio::test]
async fn test_password_login_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();

    // Create argon2 hash for "testpass"
    use argon2::password_hash::rand_core::OsRng;
    use argon2::password_hash::SaltString;
    use argon2::{Argon2, PasswordHasher};
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(b"testpass", &salt)
        .unwrap()
        .to_string();

    let user = User {
        id: Uuid::new_v4(),
        tenant_id,
        google_sub: None,
        lineworks_id: None,
        line_user_id: None,
        email: "user@example.com".to_string(),
        name: "Password User".to_string(),
        role: "admin".to_string(),
        username: Some("testuser".to_string()),
        password_hash: Some(hash),
        refresh_token_hash: None,
        refresh_token_expires_at: None,
        created_at: chrono::Utc::now(),
    };

    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_username_user.lock().unwrap() = Some(user);
    *mock.return_slug.lock().unwrap() = Some("test-org".to_string());

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/login"))
        .json(&serde_json::json!({
            "organization_id": tenant_id.to_string(),
            "username": "testuser",
            "password": "testpass"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["access_token"].is_string());
    assert!(body["refresh_token"].is_string());
    assert_eq!(body["expires_in"], 3600);
    assert_eq!(body["user"]["email"], "user@example.com");
    assert_eq!(body["user"]["name"], "Password User");
}

// ============================================================
// POST /api/auth/login — wrong password → 401
// ============================================================

#[tokio::test]
async fn test_password_login_wrong_password() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();

    use argon2::password_hash::rand_core::OsRng;
    use argon2::password_hash::SaltString;
    use argon2::{Argon2, PasswordHasher};
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(b"correct-pass", &salt)
        .unwrap()
        .to_string();

    let user = User {
        id: Uuid::new_v4(),
        tenant_id,
        google_sub: None,
        lineworks_id: None,
        line_user_id: None,
        email: "user@example.com".to_string(),
        name: "Password User".to_string(),
        role: "admin".to_string(),
        username: Some("testuser".to_string()),
        password_hash: Some(hash),
        refresh_token_hash: None,
        refresh_token_expires_at: None,
        created_at: chrono::Utc::now(),
    };

    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_username_user.lock().unwrap() = Some(user);

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/login"))
        .json(&serde_json::json!({
            "organization_id": tenant_id.to_string(),
            "username": "testuser",
            "password": "wrong-pass"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

// ============================================================
// POST /api/auth/login — user not found → 401
// ============================================================

#[tokio::test]
async fn test_password_login_user_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();

    // return_username_user = None → user not found → 401
    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/login"))
        .json(&serde_json::json!({
            "organization_id": tenant_id.to_string(),
            "username": "nonexistent",
            "password": "any"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

// ============================================================
// POST /api/auth/login — no password_hash set → 401
// ============================================================

#[tokio::test]
async fn test_password_login_no_hash() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();

    // User exists but password_hash is None
    let user = User {
        id: Uuid::new_v4(),
        tenant_id,
        google_sub: None,
        lineworks_id: None,
        line_user_id: None,
        email: "user@example.com".to_string(),
        name: "No Hash User".to_string(),
        role: "admin".to_string(),
        username: Some("testuser".to_string()),
        password_hash: None,
        refresh_token_hash: None,
        refresh_token_expires_at: None,
        created_at: chrono::Utc::now(),
    };

    let mock = Arc::new(MockAuthRepository::default());
    *mock.return_username_user.lock().unwrap() = Some(user);

    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/login"))
        .json(&serde_json::json!({
            "organization_id": tenant_id.to_string(),
            "username": "testuser",
            "password": "any"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

// ============================================================
// POST /api/auth/login — invalid organization_id → 400
// ============================================================

#[tokio::test]
async fn test_password_login_invalid_org_id() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockAuthRepository::default());
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/login"))
        .json(&serde_json::json!({
            "organization_id": "not-a-uuid",
            "username": "testuser",
            "password": "any"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

// ============================================================
// POST /api/auth/login — DB error on find_user_by_username
// ============================================================

#[tokio::test]
async fn test_password_login_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let tenant_id = Uuid::new_v4();

    let mock = Arc::new(MockAuthRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.auth = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/auth/login"))
        .json(&serde_json::json!({
            "organization_id": tenant_id.to_string(),
            "username": "testuser",
            "password": "any"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// ============================================================
// line_callback — existing user → redirect with JWT (wiremock)
// ============================================================

#[tokio::test]
async fn test_line_callback_existing_user() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    let mock_server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/oauth2/v2.1/token"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"access_token": "t"})),
        )
        .mount(&mock_server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v2/profile"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"userId": "lu", "displayName": "T"})),
        )
        .mount(&mock_server)
        .await;

    let tid = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
    let mock_auth = Arc::new(MockAuthRepository::default());
    *mock_auth.return_line_user.lock().unwrap() = Some(User {
        id: Uuid::new_v4(),
        tenant_id: tid,
        email: "l@e.com".into(),
        name: "T".into(),
        role: "admin".into(),
        google_sub: None,
        lineworks_id: None,
        line_user_id: Some("lu".into()),
        username: None,
        password_hash: None,
        refresh_token_hash: None,
        refresh_token_expires_at: None,
        created_at: chrono::Utc::now(),
    });
    *mock_auth.return_slug.lock().unwrap() = Some("s".into());
    let mut state = setup_mock_app_state();
    state.auth = mock_auth;
    let base_url = crate::common::spawn_test_server(state).await;

    let ss = "test-ss1";
    std::env::set_var("OAUTH_STATE_SECRET", ss);
    std::env::set_var("LINE_LOGIN_CHANNEL_ID", "c");
    std::env::set_var("LINE_LOGIN_CHANNEL_SECRET", "s");
    std::env::set_var("LINE_API_BASE", mock_server.uri());
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let p = lineworks::state::StatePayload {
        redirect_uri: "https://e.com/cb".into(),
        nonce: "n".into(),
        provider: "line".into(),
        external_org_id: String::new(),
    };
    let signed = lineworks::state::sign(&p, ss);
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/line/callback?code=c&state={}",
            urlencoding::encode(&signed)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 307);
    assert!(res
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("token="));
    std::env::remove_var("LINE_API_BASE");
}

// ============================================================
// line_callback — 0 tenants → error redirect
// ============================================================

#[tokio::test]
async fn test_line_callback_zero_tenants() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    let mock_server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/oauth2/v2.1/token"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"access_token": "t"})),
        )
        .mount(&mock_server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v2/profile"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"userId": "u0", "displayName": "U"})),
        )
        .mount(&mock_server)
        .await;

    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let ss = "ss2";
    std::env::set_var("OAUTH_STATE_SECRET", ss);
    std::env::set_var("LINE_LOGIN_CHANNEL_ID", "c");
    std::env::set_var("LINE_LOGIN_CHANNEL_SECRET", "s");
    std::env::set_var("LINE_API_BASE", mock_server.uri());
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let p = lineworks::state::StatePayload {
        redirect_uri: "https://e.com/cb".into(),
        nonce: "n".into(),
        provider: "line".into(),
        external_org_id: String::new(),
    };
    let signed = lineworks::state::sign(&p, ss);
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/line/callback?code=c&state={}",
            urlencoding::encode(&signed)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 307);
    assert!(res
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("error="));
    std::env::remove_var("LINE_API_BASE");
}

// ============================================================
// line_callback — 1 tenant → auto login
// ============================================================

#[tokio::test]
async fn test_line_callback_one_tenant() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    let mock_server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/oauth2/v2.1/token"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"access_token": "t"})),
        )
        .mount(&mock_server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v2/profile"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"userId": "u1", "displayName": "N"})),
        )
        .mount(&mock_server)
        .await;

    let tid = Uuid::new_v4();
    let mock_auth = Arc::new(MockAuthRepository::default());
    *mock_auth.return_line_recipients.lock().unwrap() = vec![(tid, "Org".into())];
    *mock_auth.return_slug.lock().unwrap() = Some("s".into());
    let mut state = setup_mock_app_state();
    state.auth = mock_auth;
    let base_url = crate::common::spawn_test_server(state).await;

    let ss = "ss3";
    std::env::set_var("OAUTH_STATE_SECRET", ss);
    std::env::set_var("LINE_LOGIN_CHANNEL_ID", "c");
    std::env::set_var("LINE_LOGIN_CHANNEL_SECRET", "s");
    std::env::set_var("LINE_API_BASE", mock_server.uri());
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let p = lineworks::state::StatePayload {
        redirect_uri: "https://e.com/cb".into(),
        nonce: "n".into(),
        provider: "line".into(),
        external_org_id: String::new(),
    };
    let signed = lineworks::state::sign(&p, ss);
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/line/callback?code=c&state={}",
            urlencoding::encode(&signed)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 307);
    assert!(res
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("token="));
    std::env::remove_var("LINE_API_BASE");
}

// ============================================================
// line_callback — multiple tenants → selection redirect
// ============================================================

#[tokio::test]
async fn test_line_callback_multiple_tenants() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    let mock_server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/oauth2/v2.1/token"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"access_token": "t"})),
        )
        .mount(&mock_server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v2/profile"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"userId": "um", "displayName": "M"})),
        )
        .mount(&mock_server)
        .await;

    let mock_auth = Arc::new(MockAuthRepository::default());
    *mock_auth.return_line_recipients.lock().unwrap() =
        vec![(Uuid::new_v4(), "A".into()), (Uuid::new_v4(), "B".into())];
    let mut state = setup_mock_app_state();
    state.auth = mock_auth;
    let base_url = crate::common::spawn_test_server(state).await;

    let ss = "ss4";
    std::env::set_var("OAUTH_STATE_SECRET", ss);
    std::env::set_var("LINE_LOGIN_CHANNEL_ID", "c");
    std::env::set_var("LINE_LOGIN_CHANNEL_SECRET", "s");
    std::env::set_var("LINE_API_BASE", mock_server.uri());
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let p = lineworks::state::StatePayload {
        redirect_uri: "https://e.com/s".into(),
        nonce: "n".into(),
        provider: "line".into(),
        external_org_id: String::new(),
    };
    let signed = lineworks::state::sign(&p, ss);
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/line/callback?code=c&state={}",
            urlencoding::encode(&signed)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 307);
    assert!(res
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("tenants="));
    std::env::remove_var("LINE_API_BASE");
}

// ============================================================
// line_callback — invalid state → 400
// ============================================================

#[tokio::test]
async fn test_line_callback_invalid_state_param() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("OAUTH_STATE_SECRET", "s");
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/line/callback?code=c&state=bad"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

// ============================================================
// line_callback — token exchange fails → 502
// ============================================================

#[tokio::test]
async fn test_line_callback_token_exchange_fails() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    let mock_server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/oauth2/v2.1/token"))
        .respond_with(wiremock::ResponseTemplate::new(401).set_body_string("fail"))
        .mount(&mock_server)
        .await;

    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let ss = "ss5";
    std::env::set_var("OAUTH_STATE_SECRET", ss);
    std::env::set_var("LINE_LOGIN_CHANNEL_ID", "c");
    std::env::set_var("LINE_LOGIN_CHANNEL_SECRET", "s");
    std::env::set_var("LINE_API_BASE", mock_server.uri());
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let p = lineworks::state::StatePayload {
        redirect_uri: "https://e.com/cb".into(),
        nonce: "n".into(),
        provider: "line".into(),
        external_org_id: String::new(),
    };
    let signed = lineworks::state::sign(&p, ss);
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/line/callback?code=c&state={}",
            urlencoding::encode(&signed)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 502);
    std::env::remove_var("LINE_API_BASE");
}

// ============================================================
// line_callback — profile fetch fails → 502
// ============================================================

#[tokio::test]
async fn test_line_callback_profile_fetch_fails() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    let mock_server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/oauth2/v2.1/token"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"access_token": "t"})),
        )
        .mount(&mock_server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v2/profile"))
        .respond_with(wiremock::ResponseTemplate::new(500).set_body_string("fail"))
        .mount(&mock_server)
        .await;

    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let ss = "ss6";
    std::env::set_var("OAUTH_STATE_SECRET", ss);
    std::env::set_var("LINE_LOGIN_CHANNEL_ID", "c");
    std::env::set_var("LINE_LOGIN_CHANNEL_SECRET", "s");
    std::env::set_var("LINE_API_BASE", mock_server.uri());
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let p = lineworks::state::StatePayload {
        redirect_uri: "https://e.com/cb".into(),
        nonce: "n".into(),
        provider: "line".into(),
        external_org_id: String::new(),
    };
    let signed = lineworks::state::sign(&p, ss);
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/line/callback?code=c&state={}",
            urlencoding::encode(&signed)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 502);
    std::env::remove_var("LINE_API_BASE");
}

// ============================================================
// line_callback — missing LINE_LOGIN_CHANNEL_ID → 500
// ============================================================

#[tokio::test]
async fn test_line_callback_missing_channel_id() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    let ss = "ss-ch-missing";
    std::env::set_var("OAUTH_STATE_SECRET", ss);
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::remove_var("LINE_LOGIN_CHANNEL_ID");
    std::env::remove_var("LINE_LOGIN_CHANNEL_SECRET");

    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let p = lineworks::state::StatePayload {
        redirect_uri: "https://e.com/cb".into(),
        nonce: "n".into(),
        provider: "line".into(),
        external_org_id: String::new(),
    };
    let signed = lineworks::state::sign(&p, ss);
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/line/callback?code=c&state={}",
            urlencoding::encode(&signed)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// line_callback — QR invite with external_org_id → redirect
// ============================================================

#[tokio::test]
async fn test_line_callback_qr_invite() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    let mock_server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/oauth2/v2.1/token"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"access_token": "t"})),
        )
        .mount(&mock_server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/v2/profile"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"userId": "qr-user", "displayName": "QR"})),
        )
        .mount(&mock_server)
        .await;

    let tid = Uuid::new_v4();
    let mock_auth = Arc::new(MockAuthRepository::default());
    *mock_auth.return_slug.lock().unwrap() = Some("s".into());
    let mut state = setup_mock_app_state();
    state.auth = mock_auth;
    let base_url = crate::common::spawn_test_server(state).await;

    let ss = "ss-qr";
    std::env::set_var("OAUTH_STATE_SECRET", ss);
    std::env::set_var("LINE_LOGIN_CHANNEL_ID", "c");
    std::env::set_var("LINE_LOGIN_CHANNEL_SECRET", "s");
    std::env::set_var("LINE_API_BASE", mock_server.uri());
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let p = lineworks::state::StatePayload {
        redirect_uri: "https://e.com/cb".into(),
        nonce: "n".into(),
        provider: "line".into(),
        external_org_id: tid.to_string(),
    };
    let signed = lineworks::state::sign(&p, ss);
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let res = client
        .get(format!(
            "{base_url}/api/auth/line/callback?code=c&state={}",
            urlencoding::encode(&signed)
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 307);
    assert!(res
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("token="));
    std::env::remove_var("LINE_API_BASE");
}

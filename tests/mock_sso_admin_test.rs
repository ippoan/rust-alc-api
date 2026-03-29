mod common;
mod mock_helpers;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use mock_helpers::MockSsoAdminRepository;

/// Helper: set up mock AppState and spawn test server with admin JWT.
/// Returns (base_url, auth_header).
async fn setup() -> (String, String) {
    let state = mock_helpers::app_state::setup_mock_app_state().await;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header)
}

/// Helper: set up with a failing mock for sso_admin, returning (base_url, auth_header).
async fn setup_failing() -> (String, String) {
    let mock = Arc::new(MockSsoAdminRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = mock_helpers::app_state::setup_mock_app_state().await;
    state.sso_admin = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header)
}

// =========================================================================
// GET /api/admin/sso/configs
// =========================================================================

#[tokio::test]
async fn test_list_configs_success() {
    let (base_url, auth_header) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["configs"].is_array());
    assert_eq!(body["configs"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_list_configs_forbidden_for_viewer() {
    let state = mock_helpers::app_state::setup_mock_app_state().await;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "viewer");
    let auth_header = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn test_list_configs_no_auth() {
    let state = mock_helpers::app_state::setup_mock_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/admin/sso/configs"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_list_configs_db_error() {
    let (base_url, auth_header) = setup_failing().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =========================================================================
// POST /api/admin/sso/configs — upsert (with client_secret)
// =========================================================================

#[tokio::test]
async fn test_upsert_config_with_secret_success() {
    let (base_url, auth_header) = setup().await;
    let client = reqwest::Client::new();

    // Need JWT_SECRET or SSO_ENCRYPTION_KEY for encryption
    let _lock = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", "test-encryption-key-for-sso");

    let res = client
        .post(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "provider": "lineworks",
            "client_id": "test-client-id",
            "client_secret": "test-secret-value",
            "external_org_id": "test-org-123",
            "woff_id": "woff-abc",
            "enabled": true
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["provider"], "lineworks");
    assert_eq!(body["client_id"], "test-client-id");
    assert_eq!(body["external_org_id"], "test-org-123");
    assert_eq!(body["woff_id"], "woff-abc");
    assert_eq!(body["enabled"], true);
}

#[tokio::test]
async fn test_upsert_config_with_empty_secret() {
    let (base_url, auth_header) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "provider": "lineworks",
            "client_id": "test-client-id",
            "client_secret": "",
            "external_org_id": "test-org-456"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["provider"], "lineworks");
    assert_eq!(body["enabled"], true); // default
}

// =========================================================================
// POST /api/admin/sso/configs — upsert (without client_secret)
// =========================================================================

#[tokio::test]
async fn test_upsert_config_without_secret_success() {
    let (base_url, auth_header) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "provider": "lineworks",
            "client_id": "test-client-id",
            "external_org_id": "test-org-789",
            "enabled": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["provider"], "lineworks");
    assert_eq!(body["client_id"], "test-client-id");
    assert_eq!(body["external_org_id"], "test-org-789");
    assert_eq!(body["enabled"], false);
    assert!(body["woff_id"].is_null());
}

#[tokio::test]
async fn test_upsert_config_with_woff_id() {
    let (base_url, auth_header) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "provider": "lineworks",
            "client_id": "cl-id",
            "external_org_id": "org-id",
            "woff_id": "my-woff-id"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["woff_id"], "my-woff-id");
}

#[tokio::test]
async fn test_upsert_config_forbidden_for_viewer() {
    let state = mock_helpers::app_state::setup_mock_app_state().await;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "viewer");
    let auth_header = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "provider": "lineworks",
            "client_id": "cl",
            "external_org_id": "org"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn test_upsert_config_no_auth() {
    let state = mock_helpers::app_state::setup_mock_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/admin/sso/configs"))
        .json(&serde_json::json!({
            "provider": "lineworks",
            "client_id": "cl",
            "external_org_id": "org"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_upsert_config_without_secret_db_error() {
    let (base_url, auth_header) = setup_failing().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "provider": "lineworks",
            "client_id": "cl",
            "external_org_id": "org"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_upsert_config_with_secret_db_error() {
    let mock = Arc::new(MockSsoAdminRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = mock_helpers::app_state::setup_mock_app_state().await;
    state.sso_admin = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let _lock = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", "test-encryption-key-for-sso");

    let res = client
        .post(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "provider": "lineworks",
            "client_id": "cl",
            "client_secret": "some-secret",
            "external_org_id": "org"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_upsert_config_with_secret_no_encryption_key() {
    let (base_url, auth_header) = setup().await;
    let client = reqwest::Client::new();

    // Remove both SSO_ENCRYPTION_KEY and JWT_SECRET to trigger 500
    let _lock = common::ENV_LOCK.lock().unwrap();
    std::env::remove_var("SSO_ENCRYPTION_KEY");
    std::env::remove_var("JWT_SECRET");

    let res = client
        .post(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "provider": "lineworks",
            "client_id": "cl",
            "client_secret": "secret-value",
            "external_org_id": "org"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =========================================================================
// DELETE /api/admin/sso/configs
// =========================================================================

#[tokio::test]
async fn test_delete_config_success() {
    let (base_url, auth_header) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .delete(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({ "provider": "lineworks" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_delete_config_forbidden_for_viewer() {
    let state = mock_helpers::app_state::setup_mock_app_state().await;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "viewer");
    let auth_header = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let res = client
        .delete(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({ "provider": "lineworks" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn test_delete_config_no_auth() {
    let state = mock_helpers::app_state::setup_mock_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .delete(format!("{base_url}/api/admin/sso/configs"))
        .json(&serde_json::json!({ "provider": "lineworks" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_delete_config_db_error() {
    let (base_url, auth_header) = setup_failing().await;
    let client = reqwest::Client::new();

    let res = client
        .delete(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({ "provider": "lineworks" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

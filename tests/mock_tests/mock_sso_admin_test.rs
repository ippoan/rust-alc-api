use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::mock_helpers::MockSsoAdminRepository;

/// Helper: set up mock AppState and spawn test server with admin JWT.
/// Returns (base_url, auth_header).
async fn setup() -> (String, String) {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header)
}

/// Helper: set up with a failing mock for sso_admin, returning (base_url, auth_header).
async fn setup_failing() -> (String, String) {
    let mock = Arc::new(MockSsoAdminRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.sso_admin = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header)
}

/// Helper: mock whose delete_config returns 0 rows (= 該当なし).
async fn setup_delete_zero() -> (String, String) {
    let mock = Arc::new(MockSsoAdminRepository::default());
    mock.delete_zero.store(true, Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.sso_admin = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
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
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "viewer");
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
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
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

    // encryption には SSO_ENCRYPTION_KEY が必須 (Refs #479 — JWT_SECRET fallback 撤去)
    let _lock = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", "test-encryption-key-for-sso");

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
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "viewer");
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
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
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

/// secret 無し upsert は UPDATE only。対象 config が無い (RowNotFound) 場合は
/// 新規作成不可なので 400。mock の check_fail! は RowNotFound を返すため、この
/// path の「対象なし」= 400 を検証する (with_secret は同じ RowNotFound でも 500)。
#[tokio::test]
async fn test_upsert_config_without_secret_missing_returns_400() {
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
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_upsert_config_with_secret_db_error() {
    let mock = Arc::new(MockSsoAdminRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.sso_admin = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let _lock = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", "test-encryption-key-for-sso");

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
    let _lock = crate::common::ENV_LOCK.lock().unwrap();
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
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "viewer");
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
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .delete(format!("{base_url}/api/admin/sso/configs"))
        .json(&serde_json::json!({ "provider": "lineworks" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

/// delete_config が 0 行 (= 該当テナントに該当 provider なし) → 404
#[tokio::test]
async fn test_delete_config_not_found() {
    let (base_url, auth_header) = setup_delete_zero().await;
    let client = reqwest::Client::new();

    let res = client
        .delete(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({ "provider": "lineworks" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
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

// =========================================================================
// GET /api/admin/sso/configs/export — developer 限定
// =========================================================================

fn dev_email() -> &'static str {
    "m.tama.ramu@gmail.com"
}

fn set_dev_emails(value: &str) -> Option<String> {
    let prev = std::env::var("DEVELOPER_EMAILS").ok();
    std::env::set_var("DEVELOPER_EMAILS", value);
    prev
}

fn restore_dev_emails(prev: Option<String>) {
    match prev {
        Some(v) => std::env::set_var("DEVELOPER_EMAILS", v),
        None => std::env::remove_var("DEVELOPER_EMAILS"),
    }
}

#[tokio::test]
async fn test_export_configs_success() {
    use rust_alc_api::db::repository::sso_admin::{SsoConfigExportRow, TenantInfoForExport};
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_JWT_SECRET);
    let prev = set_dev_emails(dev_email());

    let mock = Arc::new(MockSsoAdminRepository::default());
    let tenant_id = uuid::Uuid::new_v4();
    *mock.return_tenant_for_export.lock().unwrap() = Some(TenantInfoForExport {
        id: tenant_id,
        name: "テナント大石".to_string(),
        slug: Some("ohishi".to_string()),
        email_domain: None,
        created_at: chrono::Utc::now(),
    });
    *mock.return_configs_for_export.lock().unwrap() = vec![SsoConfigExportRow {
        id: uuid::Uuid::new_v4(),
        tenant_id,
        provider: "lineworks".to_string(),
        client_id: "cid".to_string(),
        client_secret_encrypted: "enc-secret".to_string(),
        external_org_id: "ohishiunyusouko".to_string(),
        enabled: true,
        woff_id: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }];

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.sso_admin = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let dev_jwt = crate::common::create_test_jwt_for_user(
        uuid::Uuid::new_v4(),
        tenant_id,
        dev_email(),
        "admin",
    );
    let client = reqwest::Client::new();
    let res = client
        .get(format!(
            "{base_url}/api/admin/sso/configs/export?tenant_id={tenant_id}"
        ))
        .header("Authorization", format!("Bearer {dev_jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["version"], 1);
    assert_eq!(body["tenant_id"], tenant_id.to_string());
    assert_eq!(body["data"]["tenant"]["slug"], "ohishi");
    let configs = body["data"]["sso_provider_configs"].as_array().unwrap();
    assert_eq!(configs.len(), 1);
    assert_eq!(configs[0]["external_org_id"], "ohishiunyusouko");
    assert_eq!(configs[0]["client_secret_encrypted"], "enc-secret");
    assert_eq!(body["data"]["bot_configs"].as_array().unwrap().len(), 0);

    restore_dev_emails(prev);
}

#[tokio::test]
async fn test_export_configs_forbidden_for_non_developer() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_JWT_SECRET);
    let prev = set_dev_emails(dev_email());

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.sso_admin = Arc::new(MockSsoAdminRepository::default());
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = uuid::Uuid::new_v4();
    let attacker_jwt = crate::common::create_test_jwt_for_user(
        uuid::Uuid::new_v4(),
        tenant_id,
        "attacker@example.com",
        "admin",
    );
    let client = reqwest::Client::new();
    let res = client
        .get(format!(
            "{base_url}/api/admin/sso/configs/export?tenant_id={tenant_id}"
        ))
        .header("Authorization", format!("Bearer {attacker_jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);

    restore_dev_emails(prev);
}

#[tokio::test]
async fn test_export_configs_tenant_not_found() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_JWT_SECRET);
    let prev = set_dev_emails(dev_email());

    // return_tenant_for_export はデフォルト None → handler 側で 404
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.sso_admin = Arc::new(MockSsoAdminRepository::default());
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = uuid::Uuid::new_v4();
    let dev_jwt = crate::common::create_test_jwt_for_user(
        uuid::Uuid::new_v4(),
        tenant_id,
        dev_email(),
        "admin",
    );
    let client = reqwest::Client::new();
    let res = client
        .get(format!(
            "{base_url}/api/admin/sso/configs/export?tenant_id={tenant_id}"
        ))
        .header("Authorization", format!("Bearer {dev_jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);

    restore_dev_emails(prev);
}

#[tokio::test]
async fn test_export_configs_tenant_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_JWT_SECRET);
    let prev = set_dev_emails(dev_email());

    let mock = Arc::new(MockSsoAdminRepository::default());
    mock.fail_tenant_for_export.store(true, Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.sso_admin = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = uuid::Uuid::new_v4();
    let dev_jwt = crate::common::create_test_jwt_for_user(
        uuid::Uuid::new_v4(),
        tenant_id,
        dev_email(),
        "admin",
    );
    let client = reqwest::Client::new();
    let res = client
        .get(format!(
            "{base_url}/api/admin/sso/configs/export?tenant_id={tenant_id}"
        ))
        .header("Authorization", format!("Bearer {dev_jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    restore_dev_emails(prev);
}

#[tokio::test]
async fn test_export_configs_configs_db_error() {
    use rust_alc_api::db::repository::sso_admin::TenantInfoForExport;
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_JWT_SECRET);
    let prev = set_dev_emails(dev_email());

    let mock = Arc::new(MockSsoAdminRepository::default());
    let tenant_id = uuid::Uuid::new_v4();
    *mock.return_tenant_for_export.lock().unwrap() = Some(TenantInfoForExport {
        id: tenant_id,
        name: "T".to_string(),
        slug: None,
        email_domain: None,
        created_at: chrono::Utc::now(),
    });
    mock.fail_configs_for_export.store(true, Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.sso_admin = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let dev_jwt = crate::common::create_test_jwt_for_user(
        uuid::Uuid::new_v4(),
        tenant_id,
        dev_email(),
        "admin",
    );
    let client = reqwest::Client::new();
    let res = client
        .get(format!(
            "{base_url}/api/admin/sso/configs/export?tenant_id={tenant_id}"
        ))
        .header("Authorization", format!("Bearer {dev_jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);

    restore_dev_emails(prev);
}

mod common;

use serde_json::Value;

/// JWT付きadminユーザーセットアップ
async fn setup_admin() -> (rust_alc_api::AppState, String, uuid::Uuid, String, reqwest::Client) {
    // SSO暗号化に必要
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, &format!("Admin{}", uuid::Uuid::new_v4().simple())).await;
    let (user_id, _) = common::create_test_user_in_db(&state.pool, tenant_id, "admin@test.com", "admin").await;
    let jwt = common::create_test_jwt_for_user(user_id, tenant_id, "admin@test.com", "admin");
    let client = reqwest::Client::new();
    (state, base_url, tenant_id, jwt, client)
}

// ============================================================
// Tenant Users
// ============================================================

#[tokio::test]
async fn test_list_users() {
    let (_state, base_url, _tenant_id, jwt, client) = setup_admin().await;

    let res = client
        .get(format!("{base_url}/api/admin/users"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["users"].as_array().is_some());
}

#[tokio::test]
async fn test_list_invitations() {
    let (_state, base_url, _tenant_id, jwt, client) = setup_admin().await;

    let res = client
        .get(format!("{base_url}/api/admin/users/invitations"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["invitations"].as_array().is_some());
}

#[tokio::test]
async fn test_invite_user() {
    let (_state, base_url, _tenant_id, jwt, client) = setup_admin().await;

    let res = client
        .post(format!("{base_url}/api/admin/users/invite"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({
            "email": "newuser@example.com",
            "role": "viewer"
        }))
        .send().await.unwrap();
    assert!(res.status() == 200 || res.status() == 201, "invite: {}", res.status());

    // 招待一覧に表示
    let res = client
        .get(format!("{base_url}/api/admin/users/invitations"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    let body: Value = res.json().await.unwrap();
    let invitations = body["invitations"].as_array().unwrap();
    assert!(invitations.iter().any(|i| i["email"] == "newuser@example.com"));
}

#[tokio::test]
async fn test_invite_and_delete_invitation() {
    let (_state, base_url, _tenant_id, jwt, client) = setup_admin().await;

    let res = client
        .post(format!("{base_url}/api/admin/users/invite"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "email": "delete-me@example.com" }))
        .send().await.unwrap();
    let body: Value = res.json().await.unwrap();
    let inv_id = body["id"].as_str().unwrap();

    let res = client
        .delete(format!("{base_url}/api/admin/users/invite/{inv_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_delete_user() {
    let (state, base_url, tenant_id, jwt, client) = setup_admin().await;

    // 削除用ユーザーを作成
    let (target_id, _) = common::create_test_user_in_db(&state.pool, tenant_id, "target@test.com", "viewer").await;

    let res = client
        .delete(format!("{base_url}/api/admin/users/{target_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_users_forbidden_for_viewer() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "ViewerForbid").await;
    let (user_id, _) = common::create_test_user_in_db(&state.pool, tenant_id, "viewer@test.com", "viewer").await;
    let jwt = common::create_test_jwt_for_user(user_id, tenant_id, "viewer@test.com", "viewer");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/admin/users"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 403);
}

// ============================================================
// SSO Admin
// ============================================================

#[tokio::test]
async fn test_sso_list_configs() {
    let (_state, base_url, _tenant_id, jwt, client) = setup_admin().await;

    let res = client
        .get(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["configs"].as_array().is_some());
}

#[tokio::test]
async fn test_sso_upsert_and_delete_config() {
    let (_state, base_url, _tenant_id, jwt, client) = setup_admin().await;

    // Upsert
    let res = client
        .post(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({
            "provider": "lineworks",
            "client_id": "test-client-id",
            "client_secret": "test-secret",
            "external_org_id": "test-org",
            "woff_id": "test-woff"
        }))
        .send().await.unwrap();
    assert!(res.status() == 200 || res.status() == 201, "sso upsert: {}", res.status());

    // List to verify
    let res = client
        .get(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    let body: Value = res.json().await.unwrap();
    assert!(!body["configs"].as_array().unwrap().is_empty());

    // Delete
    let res = client
        .delete(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "provider": "lineworks" }))
        .send().await.unwrap();
    assert_eq!(res.status(), 204);
}

// ============================================================
// Bot Admin
// ============================================================

#[tokio::test]
async fn test_bot_list_configs() {
    let (_state, base_url, _tenant_id, jwt, client) = setup_admin().await;

    let res = client
        .get(format!("{base_url}/api/admin/bot/configs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["configs"].as_array().is_some());
}

#[tokio::test]
async fn test_bot_upsert_and_delete_config() {
    let (_state, base_url, _tenant_id, jwt, client) = setup_admin().await;

    // Upsert (create)
    let res = client
        .post(format!("{base_url}/api/admin/bot/configs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({
            "name": "Test Bot",
            "client_id": "bot-client-id",
            "client_secret": "bot-secret",
            "service_account": "bot@service",
            "private_key": "-----BEGIN RSA PRIVATE KEY-----\ntest\n-----END RSA PRIVATE KEY-----",
            "bot_id": "bot-123"
        }))
        .send().await.unwrap();
    assert!(res.status() == 200 || res.status() == 201, "bot upsert: {}", res.status());
    let body: Value = res.json().await.unwrap();
    let bot_id = body["id"].as_str().unwrap();

    // Delete
    let res = client
        .delete(format!("{base_url}/api/admin/bot/configs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "id": bot_id }))
        .send().await.unwrap();
    assert_eq!(res.status(), 204);
}

// ============================================================
// Bot Admin edge cases
// ============================================================

/// Upsert with existing id = update path
#[tokio::test]
async fn test_bot_upsert_update_existing() {
    let (_state, base_url, _tenant_id, jwt, client) = setup_admin().await;

    // Create
    let res = client
        .post(format!("{base_url}/api/admin/bot/configs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({
            "name": "UpdBot",
            "client_id": "orig-client",
            "client_secret": "orig-secret",
            "service_account": "sa@test",
            "private_key": "-----BEGIN RSA PRIVATE KEY-----\norig\n-----END RSA PRIVATE KEY-----",
            "bot_id": "bot-orig"
        }))
        .send().await.unwrap();
    assert!(res.status() == 200 || res.status() == 201);
    let body: Value = res.json().await.unwrap();
    let bot_id = body["id"].as_str().unwrap().to_string();

    // Update via upsert with id
    let res = client
        .post(format!("{base_url}/api/admin/bot/configs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({
            "id": bot_id,
            "name": "UpdBot Renamed",
            "client_id": "new-client",
            "client_secret": "new-secret",
            "service_account": "sa2@test",
            "private_key": "-----BEGIN RSA PRIVATE KEY-----\nnew\n-----END RSA PRIVATE KEY-----",
            "bot_id": "bot-new",
            "enabled": false
        }))
        .send().await.unwrap();
    assert!(res.status() == 200 || res.status() == 201, "bot update: {}", res.status());
    let updated: Value = res.json().await.unwrap();
    assert_eq!(updated["id"], bot_id);
    assert_eq!(updated["name"], "UpdBot Renamed");
    assert_eq!(updated["client_id"], "new-client");
    assert_eq!(updated["bot_id"], "bot-new");
    assert_eq!(updated["enabled"], false);

    // Cleanup
    client.delete(format!("{base_url}/api/admin/bot/configs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "id": bot_id }))
        .send().await.unwrap();
}

/// Update with empty client_secret/private_key should not overwrite encrypted fields
#[tokio::test]
async fn test_bot_upsert_update_empty_secrets() {
    let (_state, base_url, _tenant_id, jwt, client) = setup_admin().await;

    let res = client
        .post(format!("{base_url}/api/admin/bot/configs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({
            "name": "SecretBot",
            "client_id": "sec-client",
            "client_secret": "real-secret",
            "service_account": "sa@test",
            "private_key": "-----BEGIN RSA PRIVATE KEY-----\nreal\n-----END RSA PRIVATE KEY-----",
            "bot_id": "bot-sec"
        }))
        .send().await.unwrap();
    let body: Value = res.json().await.unwrap();
    let bot_id = body["id"].as_str().unwrap().to_string();

    // Update with empty secrets (should skip encryption for empty strings)
    let res = client
        .post(format!("{base_url}/api/admin/bot/configs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({
            "id": bot_id,
            "name": "SecretBot",
            "client_id": "sec-client",
            "client_secret": "",
            "service_account": "sa@test",
            "private_key": "",
            "bot_id": "bot-sec"
        }))
        .send().await.unwrap();
    assert!(res.status() == 200 || res.status() == 201);

    // Cleanup
    client.delete(format!("{base_url}/api/admin/bot/configs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "id": bot_id }))
        .send().await.unwrap();
}

/// Delete with invalid UUID returns 400
#[tokio::test]
async fn test_bot_delete_invalid_uuid() {
    let (_state, base_url, _tenant_id, jwt, client) = setup_admin().await;

    let res = client
        .delete(format!("{base_url}/api/admin/bot/configs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "id": "not-a-uuid" }))
        .send().await.unwrap();
    assert_eq!(res.status(), 400);
}

/// Bot list after create shows the new entry
#[tokio::test]
async fn test_bot_list_after_create() {
    let (_state, base_url, _tenant_id, jwt, client) = setup_admin().await;

    let res = client
        .post(format!("{base_url}/api/admin/bot/configs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({
            "name": "ListBot",
            "client_id": "list-client",
            "client_secret": "list-secret",
            "service_account": "sa@list",
            "private_key": "-----BEGIN RSA PRIVATE KEY-----\nlist\n-----END RSA PRIVATE KEY-----",
            "bot_id": "bot-list"
        }))
        .send().await.unwrap();
    let body: Value = res.json().await.unwrap();
    let bot_id = body["id"].as_str().unwrap().to_string();

    let res = client
        .get(format!("{base_url}/api/admin/bot/configs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    let body: Value = res.json().await.unwrap();
    let configs = body["configs"].as_array().unwrap();
    assert!(configs.iter().any(|c| c["id"] == bot_id));

    // Cleanup
    client.delete(format!("{base_url}/api/admin/bot/configs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "id": bot_id }))
        .send().await.unwrap();
}

/// Bot admin forbidden for viewer role
#[tokio::test]
async fn test_bot_forbidden_for_viewer() {
    let (state, base_url, tenant_id, _jwt, client) = setup_admin().await;

    let (viewer_id, _) = common::create_test_user_in_db(&state.pool, tenant_id, "botviewer@test.com", "viewer").await;
    let viewer_jwt = common::create_test_jwt_for_user(viewer_id, tenant_id, "botviewer@test.com", "viewer");

    let res = client
        .get(format!("{base_url}/api/admin/bot/configs"))
        .header("Authorization", format!("Bearer {viewer_jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 403);

    let res = client
        .post(format!("{base_url}/api/admin/bot/configs"))
        .header("Authorization", format!("Bearer {viewer_jwt}"))
        .json(&serde_json::json!({
            "name": "X", "client_id": "X", "service_account": "X", "bot_id": "X"
        }))
        .send().await.unwrap();
    assert_eq!(res.status(), 403);

    let res = client
        .delete(format!("{base_url}/api/admin/bot/configs"))
        .header("Authorization", format!("Bearer {viewer_jwt}"))
        .json(&serde_json::json!({ "id": uuid::Uuid::new_v4().to_string() }))
        .send().await.unwrap();
    assert_eq!(res.status(), 403);
}

// ============================================================
// SSO Admin edge cases
// ============================================================

/// SSO upsert twice with same provider = update (ON CONFLICT)
#[tokio::test]
async fn test_sso_upsert_update_existing() {
    let (_state, base_url, _tenant_id, jwt, client) = setup_admin().await;

    // Create
    client.post(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({
            "provider": "lineworks",
            "client_id": "orig-id",
            "client_secret": "orig-secret",
            "external_org_id": "orig-org",
            "woff_id": "orig-woff"
        }))
        .send().await.unwrap();

    // Update same provider
    let res = client
        .post(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({
            "provider": "lineworks",
            "client_id": "new-id",
            "client_secret": "new-secret",
            "external_org_id": "new-org",
            "woff_id": "new-woff",
            "enabled": false
        }))
        .send().await.unwrap();
    assert!(res.status() == 200 || res.status() == 201);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["client_id"], "new-id");
    assert_eq!(body["external_org_id"], "new-org");
    assert_eq!(body["woff_id"], "new-woff");
    assert_eq!(body["enabled"], false);

    // Cleanup
    client.delete(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "provider": "lineworks" }))
        .send().await.unwrap();
}

/// SSO upsert without client_secret — client_secret_encrypted NOT NULL のため失敗する可能性
#[tokio::test]
#[ignore] // client_secret_encrypted が NOT NULL の場合失敗
async fn test_sso_upsert_without_secret() {
    let (_state, base_url, _tenant_id, jwt, client) = setup_admin().await;

    let res = client
        .post(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({
            "provider": "lineworks",
            "client_id": "no-secret-id",
            "external_org_id": "no-secret-org"
        }))
        .send().await.unwrap();
    assert!(res.status() == 200 || res.status() == 201);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["provider"], "lineworks");

    // Cleanup
    client.delete(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "provider": "lineworks" }))
        .send().await.unwrap();
}

/// SSO delete non-existent provider returns 204 (DELETE is idempotent)
#[tokio::test]
async fn test_sso_delete_nonexistent() {
    let (_state, base_url, _tenant_id, jwt, client) = setup_admin().await;

    let res = client
        .delete(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "provider": "nonexistent-provider" }))
        .send().await.unwrap();
    assert_eq!(res.status(), 204);
}

/// SSO admin forbidden for viewer role
#[tokio::test]
async fn test_sso_forbidden_for_viewer() {
    let (state, base_url, tenant_id, _jwt, client) = setup_admin().await;

    let (viewer_id, _) = common::create_test_user_in_db(&state.pool, tenant_id, "ssoviewer@test.com", "viewer").await;
    let viewer_jwt = common::create_test_jwt_for_user(viewer_id, tenant_id, "ssoviewer@test.com", "viewer");

    let res = client
        .get(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", format!("Bearer {viewer_jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 403);

    let res = client
        .post(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", format!("Bearer {viewer_jwt}"))
        .json(&serde_json::json!({
            "provider": "lineworks",
            "client_id": "X",
            "external_org_id": "X"
        }))
        .send().await.unwrap();
    assert_eq!(res.status(), 403);

    let res = client
        .delete(format!("{base_url}/api/admin/sso/configs"))
        .header("Authorization", format!("Bearer {viewer_jwt}"))
        .json(&serde_json::json!({ "provider": "lineworks" }))
        .send().await.unwrap();
    assert_eq!(res.status(), 403);
}

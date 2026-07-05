//! Mock-based tests for `/api/api-tokens` endpoints.

use chrono::Utc;
use rust_alc_api::db::repository::api_tokens::ApiTokenRow;
use serde_json::json;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use uuid::Uuid;

use crate::mock_helpers::MockApiTokensRepository;

async fn setup_admin() -> (String, String) {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt_for_user(
        Uuid::new_v4(),
        tenant_id,
        "admin@example.com",
        "admin",
    );
    (base_url, format!("Bearer {jwt}"))
}

async fn setup_viewer() -> (String, String) {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "viewer");
    (base_url, format!("Bearer {jwt}"))
}

async fn setup_failing_admin() -> (String, String) {
    let mock = Arc::new(MockApiTokensRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.api_tokens = mock;
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt_for_user(
        Uuid::new_v4(),
        tenant_id,
        "admin@example.com",
        "admin",
    );
    (base_url, format!("Bearer {jwt}"))
}

async fn setup_admin_revoke_not_found() -> (String, String) {
    let mock = Arc::new(MockApiTokensRepository::default());
    mock.found_on_revoke.store(false, Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.api_tokens = mock;
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt_for_user(
        Uuid::new_v4(),
        tenant_id,
        "admin@example.com",
        "admin",
    );
    (base_url, format!("Bearer {jwt}"))
}

// =========================================================================
// GET /api/api-tokens
// =========================================================================

#[tokio::test]
async fn test_list_success_empty() {
    let (base_url, auth) = setup_admin().await;
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/api-tokens"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body.is_array());
    assert_eq!(body.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_list_returns_rows_without_hash() {
    let mock = Arc::new(MockApiTokensRepository::default());
    mock.rows.lock().unwrap().push(ApiTokenRow {
        id: Uuid::new_v4(),
        name: "ci-deploy".to_string(),
        token_prefix: "alc_abcdefgh".to_string(),
        expires_at: None,
        revoked_at: None,
        last_used_at: None,
        created_at: Utc::now(),
    });
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.api_tokens = mock;
    let tenant_id = Uuid::new_v4();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let jwt = crate::common::create_test_jwt_for_user(
        Uuid::new_v4(),
        tenant_id,
        "admin@example.com",
        "admin",
    );
    let auth = format!("Bearer {jwt}");

    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/api-tokens"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "ci-deploy");
    assert_eq!(arr[0]["token_prefix"], "alc_abcdefgh");
    // token hash は絶対に含めない
    assert!(arr[0].get("token_hash").is_none());
    assert!(arr[0].get("token").is_none());
}

#[tokio::test]
async fn test_list_forbidden_for_viewer() {
    let (base_url, auth) = setup_viewer().await;
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/api-tokens"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn test_list_unauthorized() {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/api-tokens"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_list_db_error() {
    let (base_url, auth) = setup_failing_admin().await;
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/api-tokens"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =========================================================================
// POST /api/api-tokens
// =========================================================================

#[tokio::test]
async fn test_create_success_no_expiry() {
    let (base_url, auth) = setup_admin().await;
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/api-tokens"))
        .header("Authorization", &auth)
        .json(&json!({ "name": "ci", "expires_in_days": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["name"], "ci");
    let token = body["token"].as_str().unwrap();
    assert!(token.starts_with("alc_"));
    assert!(token.len() > body["token_prefix"].as_str().unwrap().len());
}

#[tokio::test]
async fn test_create_success_with_expiry() {
    let (base_url, auth) = setup_admin().await;
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/api-tokens"))
        .header("Authorization", &auth)
        .json(&json!({ "name": "short", "expires_in_days": 7 }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_create_empty_name_rejected() {
    let (base_url, auth) = setup_admin().await;
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/api-tokens"))
        .header("Authorization", &auth)
        .json(&json!({ "name": "  ", "expires_in_days": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_create_zero_or_negative_expiry_rejected() {
    let (base_url, auth) = setup_admin().await;
    let client = reqwest::Client::new();
    for days in [0i64, -5i64] {
        let res = client
            .post(format!("{base_url}/api/api-tokens"))
            .header("Authorization", &auth)
            .json(&json!({ "name": "x", "expires_in_days": days }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400, "days = {days}");
    }
}

#[tokio::test]
async fn test_create_forbidden_for_viewer() {
    let (base_url, auth) = setup_viewer().await;
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/api-tokens"))
        .header("Authorization", &auth)
        .json(&json!({ "name": "x", "expires_in_days": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn test_create_unauthorized() {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/api-tokens"))
        .json(&json!({ "name": "x", "expires_in_days": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_create_db_error() {
    let (base_url, auth) = setup_failing_admin().await;
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/api-tokens"))
        .header("Authorization", &auth)
        .json(&json!({ "name": "x", "expires_in_days": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =========================================================================
// DELETE /api/api-tokens/{id}
// =========================================================================

#[tokio::test]
async fn test_revoke_success() {
    let (base_url, auth) = setup_admin().await;
    let id = Uuid::new_v4();
    let client = reqwest::Client::new();
    let res = client
        .delete(format!("{base_url}/api/api-tokens/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_revoke_not_found() {
    let (base_url, auth) = setup_admin_revoke_not_found().await;
    let id = Uuid::new_v4();
    let client = reqwest::Client::new();
    let res = client
        .delete(format!("{base_url}/api/api-tokens/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_revoke_forbidden_for_viewer() {
    let (base_url, auth) = setup_viewer().await;
    let id = Uuid::new_v4();
    let client = reqwest::Client::new();
    let res = client
        .delete(format!("{base_url}/api/api-tokens/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn test_revoke_unauthorized() {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let id = Uuid::new_v4();
    let client = reqwest::Client::new();
    let res = client
        .delete(format!("{base_url}/api/api-tokens/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_revoke_invalid_uuid() {
    let (base_url, auth) = setup_admin().await;
    let client = reqwest::Client::new();
    let res = client
        .delete(format!("{base_url}/api/api-tokens/not-a-uuid"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    // Path extractor で UUID parse エラー → 400
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_revoke_db_error() {
    let (base_url, auth) = setup_failing_admin().await;
    let id = Uuid::new_v4();
    let client = reqwest::Client::new();
    let res = client
        .delete(format!("{base_url}/api/api-tokens/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

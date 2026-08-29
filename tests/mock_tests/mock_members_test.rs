//! Mock-based tests for `/api/members` alias endpoints.
//!
//! 既存 tenant_users 実装を再利用し、frontend (nuxt-dtako-admin) が期待する
//! フラットな `TenantMember[]` / email キーの PATCH・DELETE を提供する。

use rust_alc_api::db::repository::tenant_users::UserRow;
use serde_json::json;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use uuid::Uuid;

use crate::mock_helpers::MockTenantUsersRepository;

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

/// Prepare a state with fail_next=true on tenant_users, admin JWT.
async fn setup_failing_admin() -> (String, String) {
    let mock = Arc::new(MockTenantUsersRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.tenant_users = mock;
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

/// Prepare a state with found_by_email=false (for 404 paths), admin JWT.
async fn setup_admin_not_found() -> (String, String) {
    let mock = Arc::new(MockTenantUsersRepository::default());
    mock.found_by_email.store(false, Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.tenant_users = mock;
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
// GET /api/members
// =========================================================================

#[tokio::test]
async fn test_list_members_success_empty() {
    let (base_url, auth) = setup_admin().await;
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/members"))
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
async fn test_list_members_merges_users_and_invitations() {
    let mock = Arc::new(MockTenantUsersRepository::default());
    mock.users.lock().unwrap().push(UserRow {
        id: Uuid::new_v4(),
        email: "alice@example.com".to_string(),
        name: "Alice".to_string(),
        role: "admin".to_string(),
        created_at: chrono::Utc::now(),
    });
    mock.invitations
        .lock()
        .unwrap()
        .push(alc_core::models::TenantAllowedEmail {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            email: "bob@example.com".to_string(),
            role: "viewer".to_string(),
            created_at: chrono::Utc::now(),
        });
    // 重複 email (users 側と同じ) — 除外される
    mock.invitations
        .lock()
        .unwrap()
        .push(alc_core::models::TenantAllowedEmail {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            email: "alice@example.com".to_string(),
            role: "viewer".to_string(),
            created_at: chrono::Utc::now(),
        });
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.tenant_users = mock;
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
        .get(format!("{base_url}/api/members"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    let emails: Vec<String> = arr
        .iter()
        .map(|v| v["email"].as_str().unwrap().to_string())
        .collect();
    assert!(emails.contains(&"alice@example.com".to_string()));
    assert!(emails.contains(&"bob@example.com".to_string()));
}

#[tokio::test]
async fn test_list_members_forbidden_for_viewer() {
    let (base_url, auth) = setup_viewer().await;
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/members"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn test_list_members_unauthorized() {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/members"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_list_members_db_error_on_users() {
    let (base_url, auth) = setup_failing_admin().await;
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/members"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =========================================================================
// POST /api/members  (invite)
// =========================================================================

#[tokio::test]
async fn test_invite_member_success_default_role() {
    let (base_url, auth) = setup_admin().await;
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/members"))
        .header("Authorization", &auth)
        .json(&json!({ "email": "new@example.com" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["email"], "new@example.com");
    assert_eq!(body["role"], "member");
}

#[tokio::test]
async fn test_invite_member_success_explicit_admin() {
    let (base_url, auth) = setup_admin().await;
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/members"))
        .header("Authorization", &auth)
        .json(&json!({ "email": "x@example.com", "role": "admin" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_invite_member_bad_role() {
    let (base_url, auth) = setup_admin().await;
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/members"))
        .header("Authorization", &auth)
        .json(&json!({ "email": "x@example.com", "role": "god" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_invite_member_forbidden_for_viewer() {
    let (base_url, auth) = setup_viewer().await;
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/members"))
        .header("Authorization", &auth)
        .json(&json!({ "email": "x@example.com" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn test_invite_member_unauthorized() {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/members"))
        .json(&json!({ "email": "x@example.com" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_invite_member_db_error() {
    let (base_url, auth) = setup_failing_admin().await;
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/members"))
        .header("Authorization", &auth)
        .json(&json!({ "email": "x@example.com" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =========================================================================
// PATCH /api/members/{email}
// =========================================================================

#[tokio::test]
async fn test_update_role_success() {
    let (base_url, auth) = setup_admin().await;
    let client = reqwest::Client::new();
    let res = client
        .patch(format!("{base_url}/api/members/foo@example.com"))
        .header("Authorization", &auth)
        .json(&json!({ "role": "viewer" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_update_role_not_found() {
    let (base_url, auth) = setup_admin_not_found().await;
    let client = reqwest::Client::new();
    let res = client
        .patch(format!("{base_url}/api/members/none@example.com"))
        .header("Authorization", &auth)
        .json(&json!({ "role": "viewer" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_update_role_bad_role() {
    let (base_url, auth) = setup_admin().await;
    let client = reqwest::Client::new();
    let res = client
        .patch(format!("{base_url}/api/members/foo@example.com"))
        .header("Authorization", &auth)
        .json(&json!({ "role": "nope" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_update_role_forbidden_for_viewer() {
    let (base_url, auth) = setup_viewer().await;
    let client = reqwest::Client::new();
    let res = client
        .patch(format!("{base_url}/api/members/foo@example.com"))
        .header("Authorization", &auth)
        .json(&json!({ "role": "admin" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn test_update_role_unauthorized() {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let client = reqwest::Client::new();
    let res = client
        .patch(format!("{base_url}/api/members/foo@example.com"))
        .json(&json!({ "role": "admin" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_update_role_db_error() {
    let (base_url, auth) = setup_failing_admin().await;
    let client = reqwest::Client::new();
    let res = client
        .patch(format!("{base_url}/api/members/foo@example.com"))
        .header("Authorization", &auth)
        .json(&json!({ "role": "admin" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =========================================================================
// DELETE /api/members/{email}
// =========================================================================

#[tokio::test]
async fn test_delete_member_success() {
    let (base_url, auth) = setup_admin().await;
    let client = reqwest::Client::new();
    let res = client
        .delete(format!("{base_url}/api/members/foo@example.com"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_delete_member_not_found() {
    let (base_url, auth) = setup_admin_not_found().await;
    let client = reqwest::Client::new();
    let res = client
        .delete(format!("{base_url}/api/members/foo@example.com"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_delete_member_self_rejected() {
    let (base_url, auth) = setup_admin().await;
    let client = reqwest::Client::new();
    // setup_admin は email = admin@example.com で JWT 発行済み
    let res = client
        .delete(format!("{base_url}/api/members/admin@example.com"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_delete_member_forbidden_for_viewer() {
    let (base_url, auth) = setup_viewer().await;
    let client = reqwest::Client::new();
    let res = client
        .delete(format!("{base_url}/api/members/foo@example.com"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn test_delete_member_unauthorized() {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let client = reqwest::Client::new();
    let res = client
        .delete(format!("{base_url}/api/members/foo@example.com"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_delete_member_db_error() {
    let (base_url, auth) = setup_failing_admin().await;
    let client = reqwest::Client::new();
    let res = client
        .delete(format!("{base_url}/api/members/foo@example.com"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_invite_member_payroll_role() {
    let (base_url, auth) = setup_admin().await;
    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/members"))
        .header("Authorization", &auth)
        .json(&json!({ "email": "x@example.com", "role": "payroll" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_update_role_payroll() {
    let (base_url, auth) = setup_admin().await;
    let client = reqwest::Client::new();
    let res = client
        .patch(format!("{base_url}/api/members/foo@example.com"))
        .header("Authorization", &auth)
        .json(&json!({ "role": "payroll" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

mod common;
mod mock_helpers;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use mock_helpers::MockTenantUsersRepository;

/// Helper: set up mock AppState and spawn test server with admin JWT.
/// Returns (base_url, auth_header, user_id, tenant_id).
async fn setup() -> (String, String, uuid::Uuid, uuid::Uuid) {
    let state = mock_helpers::app_state::setup_mock_app_state().await;
    let tenant_id = uuid::Uuid::new_v4();
    let user_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt_for_user(user_id, tenant_id, "admin@example.com", "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header, user_id, tenant_id)
}

/// Helper: set up with a failing mock for tenant_users.
async fn setup_failing() -> (String, String, uuid::Uuid, uuid::Uuid) {
    let mock = Arc::new(MockTenantUsersRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = mock_helpers::app_state::setup_mock_app_state().await;
    state.tenant_users = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let user_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt_for_user(user_id, tenant_id, "admin@example.com", "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header, user_id, tenant_id)
}

// =========================================================================
// GET /api/admin/users — list_users
// =========================================================================

#[tokio::test]
async fn test_list_users_success() {
    let (base_url, auth_header, _, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/admin/users"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["users"].is_array());
    assert_eq!(body["users"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_list_users_forbidden_for_viewer() {
    let state = mock_helpers::app_state::setup_mock_app_state().await;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "viewer");
    let auth_header = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/admin/users"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn test_list_users_no_auth() {
    let state = mock_helpers::app_state::setup_mock_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/admin/users"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_list_users_db_error() {
    let (base_url, auth_header, _, _) = setup_failing().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/admin/users"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/admin/users/invitations — list_invitations
// =========================================================================

#[tokio::test]
async fn test_list_invitations_success() {
    let (base_url, auth_header, _, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/admin/users/invitations"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["invitations"].is_array());
    assert_eq!(body["invitations"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_list_invitations_forbidden_for_viewer() {
    let state = mock_helpers::app_state::setup_mock_app_state().await;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "viewer");
    let auth_header = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/admin/users/invitations"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn test_list_invitations_no_auth() {
    let state = mock_helpers::app_state::setup_mock_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/admin/users/invitations"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_list_invitations_db_error() {
    let mock = Arc::new(MockTenantUsersRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = mock_helpers::app_state::setup_mock_app_state().await;
    state.tenant_users = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/admin/users/invitations"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =========================================================================
// POST /api/admin/users/invite — invite_user
// =========================================================================

#[tokio::test]
async fn test_invite_user_success_default_role() {
    let (base_url, auth_header, _, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/admin/users/invite"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "email": "newuser@example.com"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["email"], "newuser@example.com");
    assert_eq!(body["role"], "admin"); // default role
}

#[tokio::test]
async fn test_invite_user_success_viewer_role() {
    let (base_url, auth_header, _, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/admin/users/invite"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "email": "viewer@example.com",
            "role": "viewer"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["email"], "viewer@example.com");
    assert_eq!(body["role"], "viewer");
}

#[tokio::test]
async fn test_invite_user_success_admin_role() {
    let (base_url, auth_header, _, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/admin/users/invite"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "email": "admin2@example.com",
            "role": "admin"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["email"], "admin2@example.com");
    assert_eq!(body["role"], "admin");
}

#[tokio::test]
async fn test_invite_user_invalid_role() {
    let (base_url, auth_header, _, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/admin/users/invite"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "email": "user@example.com",
            "role": "superuser"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_invite_user_forbidden_for_viewer() {
    let state = mock_helpers::app_state::setup_mock_app_state().await;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "viewer");
    let auth_header = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/admin/users/invite"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "email": "user@example.com"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn test_invite_user_no_auth() {
    let state = mock_helpers::app_state::setup_mock_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/admin/users/invite"))
        .json(&serde_json::json!({
            "email": "user@example.com"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_invite_user_db_error() {
    let mock = Arc::new(MockTenantUsersRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = mock_helpers::app_state::setup_mock_app_state().await;
    state.tenant_users = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/admin/users/invite"))
        .header("Authorization", &auth_header)
        .json(&serde_json::json!({
            "email": "user@example.com"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =========================================================================
// DELETE /api/admin/users/invite/{id} — delete_invitation
// =========================================================================

#[tokio::test]
async fn test_delete_invitation_success() {
    let (base_url, auth_header, _, _) = setup().await;
    let client = reqwest::Client::new();
    let invitation_id = uuid::Uuid::new_v4();

    let res = client
        .delete(format!("{base_url}/api/admin/users/invite/{invitation_id}"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_delete_invitation_forbidden_for_viewer() {
    let state = mock_helpers::app_state::setup_mock_app_state().await;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "viewer");
    let auth_header = format!("Bearer {jwt}");
    let client = reqwest::Client::new();
    let id = uuid::Uuid::new_v4();

    let res = client
        .delete(format!("{base_url}/api/admin/users/invite/{id}"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn test_delete_invitation_no_auth() {
    let state = mock_helpers::app_state::setup_mock_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let id = uuid::Uuid::new_v4();

    let res = client
        .delete(format!("{base_url}/api/admin/users/invite/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_delete_invitation_db_error() {
    let mock = Arc::new(MockTenantUsersRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = mock_helpers::app_state::setup_mock_app_state().await;
    state.tenant_users = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    let client = reqwest::Client::new();
    let id = uuid::Uuid::new_v4();

    let res = client
        .delete(format!("{base_url}/api/admin/users/invite/{id}"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =========================================================================
// DELETE /api/admin/users/{id} — delete_user
// =========================================================================

#[tokio::test]
async fn test_delete_user_success() {
    let (base_url, auth_header, _, _) = setup().await;
    let client = reqwest::Client::new();
    let other_user_id = uuid::Uuid::new_v4();

    let res = client
        .delete(format!("{base_url}/api/admin/users/{other_user_id}"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_delete_user_cannot_delete_self() {
    let (base_url, auth_header, user_id, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .delete(format!("{base_url}/api/admin/users/{user_id}"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_delete_user_forbidden_for_viewer() {
    let state = mock_helpers::app_state::setup_mock_app_state().await;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "viewer");
    let auth_header = format!("Bearer {jwt}");
    let client = reqwest::Client::new();
    let id = uuid::Uuid::new_v4();

    let res = client
        .delete(format!("{base_url}/api/admin/users/{id}"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 403);
}

#[tokio::test]
async fn test_delete_user_no_auth() {
    let state = mock_helpers::app_state::setup_mock_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let id = uuid::Uuid::new_v4();

    let res = client
        .delete(format!("{base_url}/api/admin/users/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_delete_user_db_error() {
    let mock = Arc::new(MockTenantUsersRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = mock_helpers::app_state::setup_mock_app_state().await;
    state.tenant_users = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let user_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt_for_user(user_id, tenant_id, "admin@example.com", "admin");
    let auth_header = format!("Bearer {jwt}");
    let client = reqwest::Client::new();
    let other_user_id = uuid::Uuid::new_v4();

    let res = client
        .delete(format!("{base_url}/api/admin/users/{other_user_id}"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ---------------------------------------------------------------------------
// list_users with data — covers UserRow → UserResponse From impl
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_users_with_data() {
    use rust_alc_api::db::repository::tenant_users::UserRow;

    let mut state = mock_helpers::app_state::setup_mock_app_state().await;
    let tenant_id = common::create_test_tenant(&state.pool, "MockTUData").await;

    let mock = std::sync::Arc::new(mock_helpers::MockTenantUsersRepository::default());
    *mock.users.lock().unwrap() = vec![UserRow {
        id: uuid::Uuid::new_v4(),
        email: "test@example.com".to_string(),
        name: "Test User".to_string(),
        role: "admin".to_string(),
        created_at: chrono::Utc::now(),
    }];
    state.tenant_users = mock;

    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/admin/users"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    let users = body["users"].as_array().unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0]["email"], "test@example.com");
}

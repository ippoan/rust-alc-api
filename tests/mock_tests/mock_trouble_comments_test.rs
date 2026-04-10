use std::sync::Arc;
use uuid::Uuid;

use crate::mock_helpers::MockTroubleCommentsRepository;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

async fn setup() -> (String, String) {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

async fn setup_failing() -> (String, String) {
    let mock = Arc::new(MockTroubleCommentsRepository::default());
    mock.fail_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_comments = mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

// ===========================================================================
// POST /api/trouble/tickets/{ticket_id}/comments — create_comment
// ===========================================================================

#[tokio::test]
async fn create_comment_success() {
    let (base, auth) = setup().await;
    let ticket_id = Uuid::new_v4();
    let res = client()
        .post(format!("{base}/api/trouble/tickets/{ticket_id}/comments"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "body": "This is a test comment"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["body"], "This is a test comment");
}

#[tokio::test]
async fn create_comment_empty_body() {
    let (base, auth) = setup().await;
    let ticket_id = Uuid::new_v4();
    let res = client()
        .post(format!("{base}/api/trouble/tickets/{ticket_id}/comments"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "body": ""
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn create_comment_whitespace_body() {
    let (base, auth) = setup().await;
    let ticket_id = Uuid::new_v4();
    let res = client()
        .post(format!("{base}/api/trouble/tickets/{ticket_id}/comments"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "body": "   "
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn create_comment_db_error() {
    let (base, auth) = setup_failing().await;
    let ticket_id = Uuid::new_v4();
    let res = client()
        .post(format!("{base}/api/trouble/tickets/{ticket_id}/comments"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "body": "will fail"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// GET /api/trouble/tickets/{ticket_id}/comments — list_comments
// ===========================================================================

#[tokio::test]
async fn list_comments_success() {
    let (base, auth) = setup().await;
    let ticket_id = Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/trouble/tickets/{ticket_id}/comments"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body.is_array());
}

#[tokio::test]
async fn list_comments_db_error() {
    let (base, auth) = setup_failing().await;
    let ticket_id = Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/trouble/tickets/{ticket_id}/comments"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// DELETE /api/trouble/comments/{id} — delete_comment
// ===========================================================================

#[tokio::test]
async fn delete_comment_success() {
    let (base, auth) = setup().await;
    let id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/trouble/comments/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn delete_comment_db_error() {
    let mock = Arc::new(MockTroubleCommentsRepository::default());
    mock.fail_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_comments = mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/trouble/comments/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn delete_comment_returns_not_found() {
    let mock = Arc::new(MockTroubleCommentsRepository::default());
    mock.delete_returns_false
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_comments = mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/trouble/comments/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

use std::sync::Arc;
use uuid::Uuid;

use crate::mock_helpers::MockTroubleWorkflowRepository;

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
    let mock = Arc::new(MockTroubleWorkflowRepository::default());
    mock.fail_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_workflow = mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

// ===========================================================================
// GET /api/trouble/workflow/states — list_states
// ===========================================================================

#[tokio::test]
async fn list_states_success() {
    let (base, auth) = setup().await;
    let res = client()
        .get(format!("{base}/api/trouble/workflow/states"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body.is_array());
}

#[tokio::test]
async fn list_states_db_error() {
    let (base, auth) = setup_failing().await;
    let res = client()
        .get(format!("{base}/api/trouble/workflow/states"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// POST /api/trouble/workflow/states — create_state
// ===========================================================================

#[tokio::test]
async fn create_state_success() {
    let (base, auth) = setup().await;
    let res = client()
        .post(format!("{base}/api/trouble/workflow/states"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "name": "review",
            "label": "レビュー"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["name"], "review");
    assert_eq!(body["label"], "レビュー");
}

#[tokio::test]
async fn create_state_db_error() {
    let (base, auth) = setup_failing().await;
    let res = client()
        .post(format!("{base}/api/trouble/workflow/states"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "name": "review",
            "label": "レビュー"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// DELETE /api/trouble/workflow/states/{id} — delete_state
// ===========================================================================

#[tokio::test]
async fn delete_state_success() {
    let (base, auth) = setup().await;
    let id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/trouble/workflow/states/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn delete_state_db_error() {
    let mock = Arc::new(MockTroubleWorkflowRepository::default());
    mock.fail_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_workflow = mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/trouble/workflow/states/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn delete_state_not_found() {
    let mock = Arc::new(MockTroubleWorkflowRepository::default());
    mock.delete_state_returns_false
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_workflow = mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/trouble/workflow/states/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// ===========================================================================
// GET /api/trouble/workflow/transitions — list_transitions
// ===========================================================================

#[tokio::test]
async fn list_transitions_success() {
    let (base, auth) = setup().await;
    let res = client()
        .get(format!("{base}/api/trouble/workflow/transitions"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body.is_array());
}

// ===========================================================================
// POST /api/trouble/workflow/transitions — create_transition
// ===========================================================================

#[tokio::test]
async fn create_transition_success() {
    let (base, auth) = setup().await;
    let from_id = Uuid::new_v4();
    let to_id = Uuid::new_v4();
    let res = client()
        .post(format!("{base}/api/trouble/workflow/transitions"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "from_state_id": from_id,
            "to_state_id": to_id,
            "label": "テスト遷移"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["from_state_id"], from_id.to_string());
    assert_eq!(body["to_state_id"], to_id.to_string());
}

// ===========================================================================
// DELETE /api/trouble/workflow/transitions/{id} — delete_transition
// ===========================================================================

#[tokio::test]
async fn delete_transition_success() {
    let (base, auth) = setup().await;
    let id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/trouble/workflow/transitions/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

// ===========================================================================
// POST /api/trouble/workflow/setup — setup_defaults
// ===========================================================================

#[tokio::test]
async fn setup_defaults_success() {
    let (base, auth) = setup().await;
    let res = client()
        .post(format!("{base}/api/trouble/workflow/setup"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body.is_array());
    assert_eq!(body.as_array().unwrap().len(), 4);
}

// ===========================================================================
// GET /api/trouble/tickets/{ticket_id}/history — list_history
// ===========================================================================

#[tokio::test]
async fn list_history_success() {
    let (base, auth) = setup().await;
    let ticket_id = Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/trouble/tickets/{ticket_id}/history"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body.is_array());
}

#[tokio::test]
async fn list_history_db_error() {
    let (base, auth) = setup_failing().await;
    let ticket_id = Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/trouble/tickets/{ticket_id}/history"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// POST /api/trouble/workflow/states — create_state duplicate → CONFLICT
// ===========================================================================

#[tokio::test]
async fn create_state_duplicate_conflict() {
    let mock = Arc::new(MockTroubleWorkflowRepository::default());
    mock.fail_on_duplicate
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_workflow = mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let res = client()
        .post(format!("{base}/api/trouble/workflow/states"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "name": "new",
            "label": "新規"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 409);
}

// ===========================================================================
// POST /api/trouble/workflow/transitions — create_transition DB error
// ===========================================================================

#[tokio::test]
async fn create_transition_db_error() {
    let (base, auth) = setup_failing().await;
    let from_id = Uuid::new_v4();
    let to_id = Uuid::new_v4();
    let res = client()
        .post(format!("{base}/api/trouble/workflow/transitions"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "from_state_id": from_id,
            "to_state_id": to_id,
            "label": "fail"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// DELETE /api/trouble/workflow/transitions/{id} — not found
// ===========================================================================

#[tokio::test]
async fn delete_transition_not_found() {
    let mock = Arc::new(MockTroubleWorkflowRepository::default());
    mock.delete_transition_returns_false
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_workflow = mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/trouble/workflow/transitions/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn delete_transition_db_error() {
    let (base, auth) = setup_failing().await;
    let id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/trouble/workflow/transitions/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// GET /api/trouble/workflow/transitions — DB error
// ===========================================================================

#[tokio::test]
async fn list_transitions_db_error() {
    let (base, auth) = setup_failing().await;
    let res = client()
        .get(format!("{base}/api/trouble/workflow/transitions"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// POST /api/trouble/workflow/setup — DB error
// ===========================================================================

#[tokio::test]
async fn setup_defaults_db_error() {
    let (base, auth) = setup_failing().await;
    let res = client()
        .post(format!("{base}/api/trouble/workflow/setup"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

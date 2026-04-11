use std::sync::Arc;
use uuid::Uuid;

use crate::mock_helpers::MockTroubleTicketsRepository;
use crate::mock_helpers::MockTroubleWorkflowRepository;

// ---------------------------------------------------------------------------
// Helper: spawn server with default mock state
// ---------------------------------------------------------------------------

async fn setup() -> (String, String) {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

async fn setup_failing_tickets() -> (String, String) {
    let mock = Arc::new(MockTroubleTicketsRepository::default());
    mock.fail_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_tickets = mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

// ===========================================================================
// POST /api/trouble/tickets — create_ticket
// ===========================================================================

#[tokio::test]
async fn create_ticket_success() {
    let (base, auth) = setup().await;
    let res = client()
        .post(format!("{base}/api/trouble/tickets"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "category": "貨物事故",
            "title": "test ticket",
            "description": "test description"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["category"], "貨物事故");
}

#[tokio::test]
async fn create_ticket_invalid_category() {
    let (base, auth) = setup().await;
    let res = client()
        .post(format!("{base}/api/trouble/tickets"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "category": "invalid_category",
            "title": "test"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn create_ticket_db_error() {
    let (base, auth) = setup_failing_tickets().await;
    let res = client()
        .post(format!("{base}/api/trouble/tickets"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "category": "貨物事故",
            "title": "will fail"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// GET /api/trouble/tickets — list_tickets
// ===========================================================================

#[tokio::test]
async fn list_tickets_success() {
    let (base, auth) = setup().await;
    let res = client()
        .get(format!("{base}/api/trouble/tickets"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["tickets"].is_array());
    assert_eq!(body["total"], 0);
}

#[tokio::test]
async fn list_tickets_db_error() {
    let (base, auth) = setup_failing_tickets().await;
    let res = client()
        .get(format!("{base}/api/trouble/tickets"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// GET /api/trouble/tickets/{id} — get_ticket
// ===========================================================================

#[tokio::test]
async fn get_ticket_not_found() {
    let (base, auth) = setup().await;
    let id = Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/trouble/tickets/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn get_ticket_db_error() {
    let (base, auth) = setup_failing_tickets().await;
    let id = Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/trouble/tickets/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// PUT /api/trouble/tickets/{id} — update_ticket
// ===========================================================================

#[tokio::test]
async fn update_ticket_not_found() {
    let (base, auth) = setup().await;
    let id = Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/trouble/tickets/{id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "title": "updated"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn update_ticket_db_error() {
    let mock = Arc::new(MockTroubleTicketsRepository::default());
    mock.fail_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_tickets = mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let id = Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/trouble/tickets/{id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "title": "will fail"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// DELETE /api/trouble/tickets/{id} — delete_ticket
// ===========================================================================

#[tokio::test]
async fn delete_ticket_success() {
    let (base, auth) = setup().await;
    let id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/trouble/tickets/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn delete_ticket_db_error() {
    let mock = Arc::new(MockTroubleTicketsRepository::default());
    mock.fail_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_tickets = mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/trouble/tickets/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// POST /api/trouble/tickets/{id}/transition — transition_ticket
// ===========================================================================

#[tokio::test]
async fn transition_ticket_not_found() {
    let (base, auth) = setup().await;
    let id = Uuid::new_v4();
    let to_state_id = Uuid::new_v4();
    let res = client()
        .post(format!("{base}/api/trouble/tickets/{id}/transition"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "to_state_id": to_state_id
        }))
        .send()
        .await
        .unwrap();
    // get returns None => 404
    assert_eq!(res.status(), 404);
}

// ===========================================================================
// GET /api/trouble/tickets/csv — export_csv
// ===========================================================================

#[tokio::test]
async fn export_csv_success() {
    let (base, auth) = setup().await;
    let res = client()
        .get(format!("{base}/api/trouble/tickets/csv"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.contains("text/csv"));

    let cd = res
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(cd.contains("trouble_tickets.csv"));

    let bytes = res.bytes().await.unwrap();
    assert_eq!(
        &bytes[..3],
        &[0xEF, 0xBB, 0xBF],
        "CSV should start with BOM"
    );
}

// ===========================================================================
// All valid categories
// ===========================================================================

#[tokio::test]
async fn create_ticket_all_valid_categories() {
    let (base, auth) = setup().await;
    let categories = [
        "苦情・トラブル",
        "貨物事故",
        "被害事故",
        "対物事故(他損)",
        "対物事故(自損)",
        "人身事故",
        "その他",
    ];
    for cat in categories {
        let res = client()
            .post(format!("{base}/api/trouble/tickets"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "category": cat
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201, "category '{cat}' should be accepted");
    }
}

// ===========================================================================
// Transition with return_some (ticket found, transition allowed)
// ===========================================================================

#[tokio::test]
async fn transition_ticket_success() {
    let tickets_mock = Arc::new(MockTroubleTicketsRepository::default());
    tickets_mock
        .return_some
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let workflow_mock = Arc::new(MockTroubleWorkflowRepository::default());

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_tickets = tickets_mock;
    state.trouble_workflow = workflow_mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let id = Uuid::new_v4();
    let to_state_id = Uuid::new_v4();
    let res = client()
        .post(format!("{base}/api/trouble/tickets/{id}/transition"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "to_state_id": to_state_id,
            "comment": "transition comment"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

// ===========================================================================
// Transition not allowed → 422
// ===========================================================================

#[tokio::test]
async fn transition_ticket_not_allowed() {
    let tickets_mock = Arc::new(MockTroubleTicketsRepository::default());
    tickets_mock
        .return_some
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let workflow_mock = Arc::new(MockTroubleWorkflowRepository::default());
    workflow_mock
        .transition_not_allowed
        .store(true, std::sync::atomic::Ordering::SeqCst);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_tickets = tickets_mock;
    state.trouble_workflow = workflow_mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let id = Uuid::new_v4();
    let to_state_id = Uuid::new_v4();
    let res = client()
        .post(format!("{base}/api/trouble/tickets/{id}/transition"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "to_state_id": to_state_id
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 422);
}

// ===========================================================================
// Delete ticket returns false → 404
// ===========================================================================

#[tokio::test]
async fn delete_ticket_returns_false_not_found() {
    let mock = Arc::new(MockTroubleTicketsRepository::default());
    mock.delete_returns_false
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_tickets = mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/trouble/tickets/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// ===========================================================================
// Update ticket returns Some → 200
// ===========================================================================

#[tokio::test]
async fn update_ticket_success() {
    let mock = Arc::new(MockTroubleTicketsRepository::default());
    mock.return_some
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_tickets = mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let id = Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/trouble/tickets/{id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "title": "updated title"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

// ===========================================================================
// Get ticket returns Some → 200
// ===========================================================================

#[tokio::test]
async fn get_ticket_success() {
    let mock = Arc::new(MockTroubleTicketsRepository::default());
    mock.return_some
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_tickets = mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let id = Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/trouble/tickets/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

// ===========================================================================
// Create ticket with initial state + record_history (return_initial = true)
// ===========================================================================

#[tokio::test]
async fn create_ticket_with_initial_state() {
    let workflow_mock = Arc::new(MockTroubleWorkflowRepository::default());
    workflow_mock
        .return_initial
        .store(true, std::sync::atomic::Ordering::SeqCst);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_workflow = workflow_mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let res = client()
        .post(format!("{base}/api/trouble/tickets"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "category": "貨物事故",
            "title": "test with initial state"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
}

// ===========================================================================
// Create ticket with webhook
// ===========================================================================

#[tokio::test]
async fn create_ticket_with_webhook() {
    let webhook_mock = Arc::new(crate::mock_helpers::webhook::MockWebhookService::default());
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.webhook = Some(webhook_mock.clone());
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let res = client()
        .post(format!("{base}/api/trouble/tickets"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "category": "貨物事故",
            "title": "webhook test"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    assert!(
        webhook_mock.fired.load(std::sync::atomic::Ordering::SeqCst) >= 1,
        "webhook should have fired"
    );
}

// ===========================================================================
// Transition ticket with webhook
// ===========================================================================

#[tokio::test]
async fn transition_ticket_with_webhook() {
    let tickets_mock = Arc::new(MockTroubleTicketsRepository::default());
    tickets_mock
        .return_some
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let webhook_mock = Arc::new(crate::mock_helpers::webhook::MockWebhookService::default());

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_tickets = tickets_mock;
    state.webhook = Some(webhook_mock.clone());
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let id = Uuid::new_v4();
    let to_state_id = Uuid::new_v4();
    let res = client()
        .post(format!("{base}/api/trouble/tickets/{id}/transition"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "to_state_id": to_state_id
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert!(
        webhook_mock.fired.load(std::sync::atomic::Ordering::SeqCst) >= 1,
        "webhook should have fired on transition"
    );
}

// ===========================================================================
// CSV export with data (return_some = true)
// ===========================================================================

#[tokio::test]
async fn export_csv_with_data() {
    let mock = Arc::new(MockTroubleTicketsRepository::default());
    mock.return_some
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_tickets = mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let res = client()
        .get(format!("{base}/api/trouble/tickets/csv"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let bytes = res.bytes().await.unwrap();
    // BOM check
    assert_eq!(
        &bytes[..3],
        &[0xEF, 0xBB, 0xBF],
        "CSV should start with BOM"
    );
    // Should have data rows (header + 1 ticket)
    let text = String::from_utf8_lossy(&bytes[3..]);
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines.len() >= 2,
        "CSV should have header + at least 1 data row, got {}",
        lines.len()
    );
}

// ===========================================================================
// CSV export DB error
// ===========================================================================

#[tokio::test]
async fn export_csv_db_error() {
    let (base, auth) = setup_failing_tickets().await;
    let res = client()
        .get(format!("{base}/api/trouble/tickets/csv"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// get_initial_state DB error → 500 on create
// ===========================================================================

#[tokio::test]
// ===========================================================================
// Update ticket with assigned_to
// ===========================================================================
#[tokio::test]
async fn update_ticket_with_assigned_to() {
    let mock = Arc::new(MockTroubleTicketsRepository::default());
    mock.return_some
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_tickets = mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let id = Uuid::new_v4();
    let assigned_to = Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/trouble/tickets/{id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "assigned_to": assigned_to
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn create_ticket_workflow_error() {
    let workflow_mock = Arc::new(MockTroubleWorkflowRepository::default());
    workflow_mock
        .fail_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_workflow = workflow_mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let res = client()
        .post(format!("{base}/api/trouble/tickets"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "category": "貨物事故",
            "title": "workflow error test"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

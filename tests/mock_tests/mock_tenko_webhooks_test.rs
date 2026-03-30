use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde_json::Value;

use crate::mock_helpers::app_state::setup_mock_app_state;
use crate::mock_helpers::MockTenkoWebhooksRepository;

// =========================================================================
// Helper: set up mock AppState and spawn test server with admin JWT.
// =========================================================================

async fn setup() -> (String, String) {
    let state = setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    (base_url, format!("Bearer {jwt}"))
}

async fn setup_with_mock(mock: Arc<MockTenkoWebhooksRepository>) -> (String, String) {
    let mut state = setup_mock_app_state();
    state.tenko_webhooks = mock;
    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    (base_url, format!("Bearer {jwt}"))
}

fn valid_webhook_body() -> serde_json::Value {
    serde_json::json!({
        "event_type": "tenko_completed",
        "url": "https://example.com/hook",
        "secret": "my-secret",
        "enabled": true
    })
}

// =========================================================================
// POST /api/tenko/webhooks — upsert
// =========================================================================

#[tokio::test]
async fn test_upsert_webhook_success() {
    let (base_url, auth) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/tenko/webhooks"))
        .header("Authorization", &auth)
        .json(&valid_webhook_body())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["event_type"], "tenko_completed");
    assert_eq!(body["url"], "https://example.com/hook");
    assert_eq!(body["enabled"], true);
    // secret is skip_serializing, should not appear
    assert!(body.get("secret").is_none() || body["secret"].is_null());
}

#[tokio::test]
async fn test_upsert_webhook_all_valid_event_types() {
    let valid_events = [
        "alcohol_detected",
        "tenko_overdue",
        "tenko_completed",
        "tenko_cancelled",
        "tenko_interrupted",
        "inspection_ng",
        "safety_judgment_fail",
        "equipment_failure",
        "report_submitted",
    ];
    for event_type in valid_events {
        let (base_url, auth) = setup().await;
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/tenko/webhooks"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "event_type": event_type,
                "url": "https://example.com/hook"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201, "Failed for event_type: {event_type}");
    }
}

#[tokio::test]
async fn test_upsert_webhook_without_secret() {
    let (base_url, auth) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/tenko/webhooks"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "event_type": "tenko_completed",
            "url": "https://example.com/hook"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["enabled"], true); // default_true
}

#[tokio::test]
async fn test_upsert_webhook_enabled_false() {
    let (base_url, auth) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/tenko/webhooks"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "event_type": "alcohol_detected",
            "url": "https://example.com/hook",
            "enabled": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["enabled"], false);
}

#[tokio::test]
async fn test_upsert_webhook_invalid_event_type() {
    let (base_url, auth) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/tenko/webhooks"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "event_type": "invalid_event",
            "url": "https://example.com/hook"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_upsert_webhook_no_auth() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/tenko/webhooks"))
        .json(&valid_webhook_body())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_upsert_webhook_with_x_tenant_id() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let tenant_id = Uuid::new_v4();
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/tenko/webhooks"))
        .header("X-Tenant-ID", tenant_id.to_string())
        .json(&valid_webhook_body())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
}

#[tokio::test]
async fn test_upsert_webhook_db_error() {
    let mock = Arc::new(MockTenkoWebhooksRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, auth) = setup_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/tenko/webhooks"))
        .header("Authorization", &auth)
        .json(&valid_webhook_body())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/tenko/webhooks — list
// =========================================================================

#[tokio::test]
async fn test_list_webhooks_success() {
    let (base_url, auth) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/webhooks"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Vec<Value> = res.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn test_list_webhooks_no_auth() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/webhooks"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_list_webhooks_with_x_tenant_id() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let tenant_id = Uuid::new_v4();
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/webhooks"))
        .header("X-Tenant-ID", tenant_id.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_list_webhooks_db_error() {
    let mock = Arc::new(MockTenkoWebhooksRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, auth) = setup_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/webhooks"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/tenko/webhooks/{id} — get
// =========================================================================

#[tokio::test]
async fn test_get_webhook_success() {
    let mock = Arc::new(MockTenkoWebhooksRepository::default());
    mock.return_found.store(true, Ordering::SeqCst);
    let (base_url, auth) = setup_with_mock(mock).await;
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/tenko/webhooks/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["event_type"], "tenko_completed");
    assert_eq!(body["url"], "https://example.com/hook");
}

#[tokio::test]
async fn test_get_webhook_not_found() {
    let (base_url, auth) = setup().await;
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/tenko/webhooks/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_get_webhook_no_auth() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/tenko/webhooks/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_get_webhook_db_error() {
    let mock = Arc::new(MockTenkoWebhooksRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, auth) = setup_with_mock(mock).await;
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/tenko/webhooks/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =========================================================================
// DELETE /api/tenko/webhooks/{id} — delete
// =========================================================================

#[tokio::test]
async fn test_delete_webhook_success() {
    let mock = Arc::new(MockTenkoWebhooksRepository::default());
    mock.return_found.store(true, Ordering::SeqCst);
    let (base_url, auth) = setup_with_mock(mock).await;
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .delete(format!("{base_url}/api/tenko/webhooks/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_delete_webhook_not_found() {
    let (base_url, auth) = setup().await;
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .delete(format!("{base_url}/api/tenko/webhooks/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_delete_webhook_no_auth() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .delete(format!("{base_url}/api/tenko/webhooks/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_delete_webhook_db_error() {
    let mock = Arc::new(MockTenkoWebhooksRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, auth) = setup_with_mock(mock).await;
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .delete(format!("{base_url}/api/tenko/webhooks/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/tenko/webhooks/{id}/deliveries — list_deliveries
// =========================================================================

#[tokio::test]
async fn test_list_deliveries_success() {
    let (base_url, auth) = setup().await;
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/tenko/webhooks/{id}/deliveries"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Vec<Value> = res.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn test_list_deliveries_no_auth() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/tenko/webhooks/{id}/deliveries"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_list_deliveries_with_x_tenant_id() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let tenant_id = Uuid::new_v4();
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/tenko/webhooks/{id}/deliveries"))
        .header("X-Tenant-ID", tenant_id.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_list_deliveries_db_error() {
    let mock = Arc::new(MockTenkoWebhooksRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, auth) = setup_with_mock(mock).await;
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/tenko/webhooks/{id}/deliveries"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

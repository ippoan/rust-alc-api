mod common;
mod mock_helpers;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use mock_helpers::app_state::setup_mock_app_state;
use mock_helpers::MockDtakoEventClassificationsRepository;
use uuid::Uuid;

use chrono::Utc;
use rust_alc_api::db::models::DtakoEventClassification;

use common::{create_test_jwt, spawn_test_server};

/// helper: build mock AppState with a shared mock repo reference
async fn setup_with_mock() -> (String, String, Arc<MockDtakoEventClassificationsRepository>) {
    let mut state = setup_mock_app_state().await;
    let mock_repo = Arc::new(MockDtakoEventClassificationsRepository::default());
    state.dtako_event_classifications = mock_repo.clone();

    let tenant_id = common::create_test_tenant(&state.pool, "ec-test").await;
    let jwt = create_test_jwt(tenant_id, "admin");
    let base_url = spawn_test_server(state).await;
    (base_url, format!("Bearer {jwt}"), mock_repo)
}

// =============================================================================
// GET /api/event-classifications
// =============================================================================

#[tokio::test]
async fn test_list_event_classifications_success() {
    let (base_url, auth, _mock) = setup_with_mock().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/event-classifications"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Vec<serde_json::Value> = res.json().await.unwrap();
    assert!(body.is_empty(), "mock returns empty vec");
}

#[tokio::test]
async fn test_list_event_classifications_db_error() {
    let (base_url, auth, mock) = setup_with_mock().await;
    let client = reqwest::Client::new();

    mock.fail_next.store(true, Ordering::SeqCst);

    let res = client
        .get(format!("{base_url}/api/event-classifications"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_list_event_classifications_no_auth() {
    let (base_url, _auth, _mock) = setup_with_mock().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/event-classifications"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

// =============================================================================
// PUT /api/event-classifications/{id}
// =============================================================================

#[tokio::test]
async fn test_update_classification_not_found() {
    let (base_url, auth, _mock) = setup_with_mock().await;
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .put(format!("{base_url}/api/event-classifications/{id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "classification": "drive" }))
        .send()
        .await
        .unwrap();

    // mock.update returns Ok(None) → 404
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_update_classification_invalid_value() {
    let (base_url, auth, _mock) = setup_with_mock().await;
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .put(format!("{base_url}/api/event-classifications/{id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "classification": "invalid_value" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
    let body = res.text().await.unwrap();
    assert!(body.contains("Invalid classification"));
    assert!(body.contains("invalid_value"));
}

#[tokio::test]
async fn test_update_classification_all_valid_values() {
    let valid = ["drive", "cargo", "rest_split", "break", "ignore"];
    let (base_url, auth, _mock) = setup_with_mock().await;
    let client = reqwest::Client::new();

    for v in valid {
        let id = Uuid::new_v4();
        let res = client
            .put(format!("{base_url}/api/event-classifications/{id}"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "classification": v }))
            .send()
            .await
            .unwrap();

        // mock returns None → 404, but NOT 400 (validation passed)
        assert_eq!(
            res.status(),
            404,
            "classification '{v}' should pass validation"
        );
    }
}

#[tokio::test]
async fn test_update_classification_db_error() {
    let (base_url, auth, mock) = setup_with_mock().await;
    let client = reqwest::Client::new();

    mock.fail_next.store(true, Ordering::SeqCst);

    let id = Uuid::new_v4();
    let res = client
        .put(format!("{base_url}/api/event-classifications/{id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "classification": "drive" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_update_classification_no_auth() {
    let (base_url, _auth, _mock) = setup_with_mock().await;
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .put(format!("{base_url}/api/event-classifications/{id}"))
        .json(&serde_json::json!({ "classification": "drive" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_update_classification_missing_body() {
    let (base_url, auth, _mock) = setup_with_mock().await;
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .put(format!("{base_url}/api/event-classifications/{id}"))
        .header("Authorization", &auth)
        .header("Content-Type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();

    // Missing "classification" field → 422 (Axum deserialization error)
    assert_eq!(res.status(), 422);
}

#[tokio::test]
async fn test_update_classification_success() {
    let mut state = setup_mock_app_state().await;
    let tenant_id = common::create_test_tenant(&state.pool, "ec-update-ok").await;

    let mock_repo = Arc::new(MockDtakoEventClassificationsRepository {
        update_result: std::sync::Mutex::new(Some(DtakoEventClassification {
            id: Uuid::new_v4(),
            tenant_id,
            event_cd: "100".to_string(),
            event_name: "出庫".to_string(),
            classification: "drive".to_string(),
            created_at: Utc::now(),
        })),
        ..Default::default()
    });
    state.dtako_event_classifications = mock_repo.clone();

    let jwt = create_test_jwt(tenant_id, "admin");
    let base_url = spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let id = Uuid::new_v4();
    let res = client
        .put(format!("{base_url}/api/event-classifications/{id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "classification": "drive" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["event_cd"], "100");
    assert_eq!(body["classification"], "drive");
}

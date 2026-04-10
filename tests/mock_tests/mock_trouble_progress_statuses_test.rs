use std::sync::Arc;
use uuid::Uuid;

use crate::mock_helpers::MockTroubleProgressStatusesRepository;

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
    let mock = Arc::new(MockTroubleProgressStatusesRepository::default());
    mock.fail_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_progress_statuses = mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

// ===========================================================================
// GET /api/trouble/progress-statuses — list_progress_statuses
// ===========================================================================

#[tokio::test]
async fn list_progress_statuses_success() {
    let (base, auth) = setup().await;
    let res = client()
        .get(format!("{base}/api/trouble/progress-statuses"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body.is_array());
}

#[tokio::test]
async fn list_progress_statuses_db_error() {
    let (base, auth) = setup_failing().await;
    let res = client()
        .get(format!("{base}/api/trouble/progress-statuses"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// POST /api/trouble/progress-statuses — create_progress_status
// ===========================================================================

#[tokio::test]
async fn create_progress_status_success() {
    let (base, auth) = setup().await;
    let res = client()
        .post(format!("{base}/api/trouble/progress-statuses"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "name": "テスト進捗",
            "sort_order": 1
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["name"], "テスト進捗");
    assert_eq!(body["sort_order"], 1);
}

#[tokio::test]
async fn create_progress_status_empty_name() {
    let (base, auth) = setup().await;
    let res = client()
        .post(format!("{base}/api/trouble/progress-statuses"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "name": "  "
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn create_progress_status_db_error() {
    let (base, auth) = setup_failing().await;
    let res = client()
        .post(format!("{base}/api/trouble/progress-statuses"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "name": "will fail"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// DELETE /api/trouble/progress-statuses/{id} — delete_progress_status
// ===========================================================================

#[tokio::test]
async fn delete_progress_status_success() {
    let (base, auth) = setup().await;
    let id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/trouble/progress-statuses/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn delete_progress_status_not_found() {
    let mock = Arc::new(MockTroubleProgressStatusesRepository::default());
    mock.delete_returns_false
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_progress_statuses = mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/trouble/progress-statuses/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn delete_progress_status_db_error() {
    let mock = Arc::new(MockTroubleProgressStatusesRepository::default());
    mock.fail_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_progress_statuses = mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/trouble/progress-statuses/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// PUT /api/trouble/progress-statuses/{id} — update_progress_status_sort
// ===========================================================================

#[tokio::test]
async fn update_progress_status_sort_order_success() {
    let statuses_mock = Arc::new(MockTroubleProgressStatusesRepository::default());
    let status_id = Uuid::new_v4();
    {
        let mut statuses = statuses_mock.statuses.lock().unwrap();
        statuses.push(rust_alc_api::db::models::TroubleProgressStatus {
            id: status_id,
            tenant_id: Uuid::nil(),
            name: "テスト進捗".to_string(),
            sort_order: 0,
            created_at: chrono::Utc::now(),
        });
    }

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_progress_statuses = statuses_mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let res = client()
        .put(format!("{base}/api/trouble/progress-statuses/{status_id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "sort_order": 5 }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["sort_order"], 5);
}

#[tokio::test]
async fn update_progress_status_sort_order_not_found() {
    let (base, auth) = setup().await;
    let id = Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/trouble/progress-statuses/{id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "sort_order": 5 }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn update_progress_status_sort_order_db_error() {
    let (base, auth) = setup_failing().await;
    let id = Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/trouble/progress-statuses/{id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "sort_order": 5 }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

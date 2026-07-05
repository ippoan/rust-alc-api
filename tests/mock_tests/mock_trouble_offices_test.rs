use std::sync::Arc;
use uuid::Uuid;

use crate::mock_helpers::MockTroubleOfficesRepository;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

async fn setup() -> (String, String) {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let base = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

async fn setup_failing() -> (String, String) {
    let mock = Arc::new(MockTroubleOfficesRepository::default());
    mock.fail_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_offices = mock;
    let base = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

// ===========================================================================
// GET /api/trouble/offices — list_offices
// ===========================================================================

#[tokio::test]
async fn list_offices_success() {
    let (base, auth) = setup().await;
    let res = client()
        .get(format!("{base}/api/trouble/offices"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body.is_array());
}

#[tokio::test]
async fn list_offices_db_error() {
    let (base, auth) = setup_failing().await;
    let res = client()
        .get(format!("{base}/api/trouble/offices"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// POST /api/trouble/offices — create_office
// ===========================================================================

#[tokio::test]
async fn create_office_success() {
    let (base, auth) = setup().await;
    let res = client()
        .post(format!("{base}/api/trouble/offices"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "name": "テスト営業所",
            "sort_order": 1
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["name"], "テスト営業所");
    assert_eq!(body["sort_order"], 1);
}

#[tokio::test]
async fn create_office_empty_name() {
    let (base, auth) = setup().await;
    let res = client()
        .post(format!("{base}/api/trouble/offices"))
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
async fn create_office_db_error() {
    let (base, auth) = setup_failing().await;
    let res = client()
        .post(format!("{base}/api/trouble/offices"))
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
// DELETE /api/trouble/offices/{id} — delete_office
// ===========================================================================

#[tokio::test]
async fn delete_office_success() {
    let (base, auth) = setup().await;
    let id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/trouble/offices/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn delete_office_not_found() {
    let mock = Arc::new(MockTroubleOfficesRepository::default());
    mock.delete_returns_false
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_offices = mock;
    let base = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let auth = format!("Bearer {jwt}");

    let id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/trouble/offices/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn delete_office_db_error() {
    let mock = Arc::new(MockTroubleOfficesRepository::default());
    mock.fail_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_offices = mock;
    let base = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let auth = format!("Bearer {jwt}");

    let id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/trouble/offices/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// PUT /api/trouble/offices/{id} — update_office_sort
// ===========================================================================

#[tokio::test]
async fn update_office_sort_order_success() {
    let offices_mock = Arc::new(MockTroubleOfficesRepository::default());
    let office_id = Uuid::new_v4();
    {
        let mut offices = offices_mock.offices.lock().unwrap();
        offices.push(rust_alc_api::db::models::TroubleOffice {
            id: office_id,
            tenant_id: Uuid::nil(),
            name: "テスト営業所".to_string(),
            sort_order: 0,
            created_at: chrono::Utc::now(),
        });
    }

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_offices = offices_mock;
    let base = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let auth = format!("Bearer {jwt}");

    let res = client()
        .put(format!("{base}/api/trouble/offices/{office_id}"))
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
async fn update_office_sort_order_not_found() {
    let (base, auth) = setup().await;
    let id = Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/trouble/offices/{id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "sort_order": 5 }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn update_office_sort_order_db_error() {
    let (base, auth) = setup_failing().await;
    let id = Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/trouble/offices/{id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "sort_order": 5 }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

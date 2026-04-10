use std::sync::Arc;
use uuid::Uuid;

use crate::mock_helpers::MockTroubleCategoriesRepository;

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
    let mock = Arc::new(MockTroubleCategoriesRepository::default());
    mock.fail_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_categories = mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

// ===========================================================================
// GET /api/trouble/categories — list_categories
// ===========================================================================

#[tokio::test]
async fn list_categories_success() {
    let (base, auth) = setup().await;
    let res = client()
        .get(format!("{base}/api/trouble/categories"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body.is_array());
}

#[tokio::test]
async fn list_categories_db_error() {
    let (base, auth) = setup_failing().await;
    let res = client()
        .get(format!("{base}/api/trouble/categories"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// POST /api/trouble/categories — create_category
// ===========================================================================

#[tokio::test]
async fn create_category_success() {
    let (base, auth) = setup().await;
    let res = client()
        .post(format!("{base}/api/trouble/categories"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "name": "テストカテゴリ",
            "sort_order": 1
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["name"], "テストカテゴリ");
    assert_eq!(body["sort_order"], 1);
}

#[tokio::test]
async fn create_category_empty_name() {
    let (base, auth) = setup().await;
    let res = client()
        .post(format!("{base}/api/trouble/categories"))
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
async fn create_category_db_error() {
    let (base, auth) = setup_failing().await;
    let res = client()
        .post(format!("{base}/api/trouble/categories"))
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
// DELETE /api/trouble/categories/{id} — delete_category
// ===========================================================================

#[tokio::test]
async fn delete_category_success() {
    let (base, auth) = setup().await;
    let id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/trouble/categories/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn delete_category_not_found() {
    let mock = Arc::new(MockTroubleCategoriesRepository::default());
    mock.delete_returns_false
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_categories = mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/trouble/categories/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn delete_category_db_error() {
    let mock = Arc::new(MockTroubleCategoriesRepository::default());
    mock.fail_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_categories = mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/trouble/categories/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// PUT /api/trouble/categories/{id} — update_category_sort
// ===========================================================================

#[tokio::test]
async fn update_category_sort_order_success() {
    let cats_mock = Arc::new(MockTroubleCategoriesRepository::default());
    let cat_id = Uuid::new_v4();
    {
        let mut cats = cats_mock.categories.lock().unwrap();
        cats.push(rust_alc_api::db::models::TroubleCategory {
            id: cat_id,
            tenant_id: Uuid::nil(),
            name: "テストカテゴリ".to_string(),
            sort_order: 0,
            created_at: chrono::Utc::now(),
        });
    }

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_categories = cats_mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let res = client()
        .put(format!("{base}/api/trouble/categories/{cat_id}"))
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
async fn update_category_sort_order_not_found() {
    let (base, auth) = setup().await;
    let id = Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/trouble/categories/{id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "sort_order": 5 }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn update_category_sort_order_db_error() {
    let (base, auth) = setup_failing().await;
    let id = Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/trouble/categories/{id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "sort_order": 5 }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// Ticket creation with DB categories
// ===========================================================================

#[tokio::test]
async fn create_ticket_with_db_category() {
    let cats_mock = Arc::new(MockTroubleCategoriesRepository::default());
    // Pre-populate a category
    {
        let mut cats = cats_mock.categories.lock().unwrap();
        cats.push(rust_alc_api::db::models::TroubleCategory {
            id: Uuid::new_v4(),
            tenant_id: Uuid::nil(),
            name: "カスタムカテゴリ".to_string(),
            sort_order: 0,
            created_at: chrono::Utc::now(),
        });
    }

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_categories = cats_mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let res = client()
        .post(format!("{base}/api/trouble/tickets"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "category": "カスタムカテゴリ"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
}

#[tokio::test]
async fn create_ticket_with_invalid_db_category() {
    let cats_mock = Arc::new(MockTroubleCategoriesRepository::default());
    // Pre-populate a category
    {
        let mut cats = cats_mock.categories.lock().unwrap();
        cats.push(rust_alc_api::db::models::TroubleCategory {
            id: Uuid::new_v4(),
            tenant_id: Uuid::nil(),
            name: "カスタムカテゴリ".to_string(),
            sort_order: 0,
            created_at: chrono::Utc::now(),
        });
    }

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_categories = cats_mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let res = client()
        .post(format!("{base}/api/trouble/tickets"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "category": "存在しないカテゴリ"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

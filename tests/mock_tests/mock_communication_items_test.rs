use uuid::Uuid;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use chrono::Utc;
use serde_json::Value;

use rust_alc_api::db::models::{
    CommunicationItem, CreateCommunicationItem, UpdateCommunicationItem,
};
use rust_alc_api::db::repository::communication_items::{
    CommunicationItemWithName, CommunicationItemsRepository,
};

// ============================================================
// SuccessMock: returns realistic data for success-path tests
// ============================================================

struct SuccessMockCommunicationItemsRepository {
    fail_next: AtomicBool,
    item_id: Uuid,
    tenant_id: Uuid,
}

impl SuccessMockCommunicationItemsRepository {
    fn new(tenant_id: Uuid) -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            item_id: Uuid::new_v4(),
            tenant_id,
        }
    }

    fn make_item(&self, title: &str) -> CommunicationItem {
        CommunicationItem {
            id: self.item_id,
            tenant_id: self.tenant_id,
            title: title.to_string(),
            content: "テスト内容".to_string(),
            priority: "normal".to_string(),
            target_employee_id: None,
            is_active: true,
            effective_from: None,
            effective_until: None,
            created_by: Some("テストユーザー".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn make_item_with_name(&self, title: &str) -> CommunicationItemWithName {
        CommunicationItemWithName {
            id: self.item_id,
            tenant_id: self.tenant_id,
            title: title.to_string(),
            content: "テスト内容".to_string(),
            priority: "normal".to_string(),
            target_employee_id: None,
            target_employee_name: None,
            is_active: true,
            effective_from: None,
            effective_until: None,
            created_by: Some("テストユーザー".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

#[async_trait::async_trait]
impl CommunicationItemsRepository for SuccessMockCommunicationItemsRepository {
    async fn list(
        &self,
        _tenant_id: Uuid,
        _is_active: Option<bool>,
        _target_employee_id: Option<Uuid>,
        _per_page: i64,
        _offset: i64,
    ) -> Result<(Vec<CommunicationItemWithName>, i64), sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok((vec![self.make_item_with_name("定期連絡")], 1))
    }

    async fn list_active(
        &self,
        _tenant_id: Uuid,
        _target_employee_id: Option<Uuid>,
    ) -> Result<Vec<CommunicationItemWithName>, sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(vec![self.make_item_with_name("有効な連絡")])
    }

    async fn get(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<CommunicationItem>, sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(Some(self.make_item("詳細連絡")))
    }

    async fn create(
        &self,
        _tenant_id: Uuid,
        input: &CreateCommunicationItem,
    ) -> Result<CommunicationItem, sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(self.make_item(&input.title))
    }

    async fn update(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        input: &UpdateCommunicationItem,
    ) -> Result<Option<CommunicationItem>, sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        let title = input.title.as_deref().unwrap_or("更新済み");
        Ok(Some(self.make_item(title)))
    }

    async fn delete(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(true)
    }
}

// ============================================================
// Helper: spawn server with a given mock
// ============================================================

async fn spawn_with_mock(mock: Arc<dyn CommunicationItemsRepository>) -> (String, String, Uuid) {
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.communication_items = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    (base_url, jwt, tenant_id)
}

fn auth(jwt: &str) -> String {
    format!("Bearer {jwt}")
}

// ============================================================
// list_items: GET /api/communication-items
// ============================================================

#[tokio::test]
async fn test_list_items_success() {
    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(SuccessMockCommunicationItemsRepository::new(tenant_id));
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/communication-items"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["items"][0]["title"], "定期連絡");
    assert_eq!(body["total"], 1);
    assert_eq!(body["page"], 1);
    assert_eq!(body["per_page"], 20);
}

#[tokio::test]
async fn test_list_items_with_filter_params() {
    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(SuccessMockCommunicationItemsRepository::new(tenant_id));
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();
    let emp_id = Uuid::new_v4();

    let res = client
        .get(format!(
            "{base_url}/api/communication-items?is_active=true&target_employee_id={emp_id}&page=2&per_page=5"
        ))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    assert_eq!(body["page"], 2);
    assert_eq!(body["per_page"], 5);
}

#[tokio::test]
async fn test_list_items_page_zero_clamped_to_one() {
    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(SuccessMockCommunicationItemsRepository::new(tenant_id));
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    // page=0 should be clamped to 1
    let res = client
        .get(format!("{base_url}/api/communication-items?page=0"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    assert_eq!(body["page"], 1);
}

#[tokio::test]
async fn test_list_items_per_page_capped_at_100() {
    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(SuccessMockCommunicationItemsRepository::new(tenant_id));
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    // per_page=200 should be capped to 100
    let res = client
        .get(format!("{base_url}/api/communication-items?per_page=200"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    assert_eq!(body["per_page"], 100);
}

#[tokio::test]
async fn test_list_items_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockCommunicationItemsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/communication-items"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// list_active_items: GET /api/communication-items/active
// ============================================================

#[tokio::test]
async fn test_list_active_items_success() {
    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(SuccessMockCommunicationItemsRepository::new(tenant_id));
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/communication-items/active"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Vec<Value> = res.json().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["title"], "有効な連絡");
}

#[tokio::test]
async fn test_list_active_items_with_target_employee() {
    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(SuccessMockCommunicationItemsRepository::new(tenant_id));
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();
    let emp_id = Uuid::new_v4();

    let res = client
        .get(format!(
            "{base_url}/api/communication-items/active?target_employee_id={emp_id}"
        ))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Vec<Value> = res.json().await.unwrap();
    assert_eq!(body.len(), 1);
}

#[tokio::test]
async fn test_list_active_items_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockCommunicationItemsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/communication-items/active"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// get_item: GET /api/communication-items/{id}
// ============================================================

#[tokio::test]
async fn test_get_item_success() {
    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(SuccessMockCommunicationItemsRepository::new(tenant_id));
    let item_id = mock.item_id;
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/communication-items/{item_id}"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    assert_eq!(body["title"], "詳細連絡");
}

#[tokio::test]
async fn test_get_item_not_found() {
    // Default mock returns Ok(None) from get
    let mock = Arc::new(crate::mock_helpers::MockCommunicationItemsRepository::default());
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();
    let fake_id = Uuid::new_v4();

    let res = client
        .get(format!("{base_url}/api/communication-items/{fake_id}"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_get_item_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockCommunicationItemsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();
    let fake_id = Uuid::new_v4();

    let res = client
        .get(format!("{base_url}/api/communication-items/{fake_id}"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// create_item: POST /api/communication-items
// ============================================================

#[tokio::test]
async fn test_create_item_success() {
    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(SuccessMockCommunicationItemsRepository::new(tenant_id));
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/communication-items"))
        .header("Authorization", auth(&jwt))
        .json(&serde_json::json!({
            "title": "新規連絡事項",
            "content": "内容です",
            "priority": "high"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);

    let body: Value = res.json().await.unwrap();
    assert_eq!(body["title"], "新規連絡事項");
}

#[tokio::test]
async fn test_create_item_minimal_body() {
    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(SuccessMockCommunicationItemsRepository::new(tenant_id));
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    // Only title is required
    let res = client
        .post(format!("{base_url}/api/communication-items"))
        .header("Authorization", auth(&jwt))
        .json(&serde_json::json!({
            "title": "最小限"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
}

#[tokio::test]
async fn test_create_item_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockCommunicationItemsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/communication-items"))
        .header("Authorization", auth(&jwt))
        .json(&serde_json::json!({
            "title": "エラーテスト"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// update_item: PUT /api/communication-items/{id}
// ============================================================

#[tokio::test]
async fn test_update_item_success() {
    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(SuccessMockCommunicationItemsRepository::new(tenant_id));
    let item_id = mock.item_id;
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .put(format!("{base_url}/api/communication-items/{item_id}"))
        .header("Authorization", auth(&jwt))
        .json(&serde_json::json!({
            "title": "更新後タイトル",
            "is_active": false
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    assert_eq!(body["title"], "更新後タイトル");
}

#[tokio::test]
async fn test_update_item_not_found() {
    // Default mock returns Ok(None) from update
    let mock = Arc::new(crate::mock_helpers::MockCommunicationItemsRepository::default());
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();
    let fake_id = Uuid::new_v4();

    let res = client
        .put(format!("{base_url}/api/communication-items/{fake_id}"))
        .header("Authorization", auth(&jwt))
        .json(&serde_json::json!({ "title": "xxx" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_update_item_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockCommunicationItemsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();
    let fake_id = Uuid::new_v4();

    let res = client
        .put(format!("{base_url}/api/communication-items/{fake_id}"))
        .header("Authorization", auth(&jwt))
        .json(&serde_json::json!({ "title": "xxx" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// delete_item: DELETE /api/communication-items/{id}
// ============================================================

#[tokio::test]
async fn test_delete_item_success() {
    let tenant_id = Uuid::new_v4();
    let mock = Arc::new(SuccessMockCommunicationItemsRepository::new(tenant_id));
    let item_id = mock.item_id;
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .delete(format!("{base_url}/api/communication-items/{item_id}"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_delete_item_not_found() {
    // Default mock returns Ok(false) from delete
    let mock = Arc::new(crate::mock_helpers::MockCommunicationItemsRepository::default());
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();
    let fake_id = Uuid::new_v4();

    let res = client
        .delete(format!("{base_url}/api/communication-items/{fake_id}"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_delete_item_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockCommunicationItemsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, jwt, _) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();
    let fake_id = Uuid::new_v4();

    let res = client
        .delete(format!("{base_url}/api/communication-items/{fake_id}"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// Auth: no JWT -> 401
// ============================================================

#[tokio::test]
async fn test_no_auth_returns_401() {
    let mock = Arc::new(crate::mock_helpers::MockCommunicationItemsRepository::default());
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.communication_items = mock;
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();
    let fake_id = Uuid::new_v4();

    // GET list
    let res = client
        .get(format!("{base_url}/api/communication-items"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // GET active
    let res = client
        .get(format!("{base_url}/api/communication-items/active"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // GET by id
    let res = client
        .get(format!("{base_url}/api/communication-items/{fake_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // POST
    let res = client
        .post(format!("{base_url}/api/communication-items"))
        .json(&serde_json::json!({ "title": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // PUT
    let res = client
        .put(format!("{base_url}/api/communication-items/{fake_id}"))
        .json(&serde_json::json!({ "title": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // DELETE
    let res = client
        .delete(format!("{base_url}/api/communication-items/{fake_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

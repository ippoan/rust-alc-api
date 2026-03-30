#[macro_use]
mod common;
mod mock_helpers;

use uuid::Uuid;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use chrono::Utc;
use serde_json::Value;

use rust_alc_api::db::models::{CarryingItem, CarryingItemVehicleCondition};
use rust_alc_api::db::repository::carrying_items::CarryingItemsRepository;

// ============================================================
// SuccessMock: returns realistic data for success-path tests
// ============================================================

struct SuccessMockCarryingItemsRepository {
    fail_next: AtomicBool,
    /// Fixed item id returned by create/update/list
    item_id: Uuid,
    tenant_id: Uuid,
}

impl SuccessMockCarryingItemsRepository {
    fn new(tenant_id: Uuid) -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            item_id: Uuid::new_v4(),
            tenant_id,
        }
    }

    fn make_item(&self, name: &str) -> CarryingItem {
        CarryingItem {
            id: self.item_id,
            tenant_id: self.tenant_id,
            item_name: name.to_string(),
            is_required: true,
            sort_order: 0,
            created_at: Utc::now(),
        }
    }

    fn make_condition(&self, category: &str, value: &str) -> CarryingItemVehicleCondition {
        CarryingItemVehicleCondition {
            id: Uuid::new_v4(),
            carrying_item_id: self.item_id,
            category: category.to_string(),
            value: value.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl CarryingItemsRepository for SuccessMockCarryingItemsRepository {
    async fn list(&self, _tenant_id: Uuid) -> Result<Vec<CarryingItem>, sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(vec![self.make_item("消火器")])
    }

    async fn list_conditions(
        &self,
        _tenant_id: Uuid,
        _item_ids: &[Uuid],
    ) -> Result<Vec<CarryingItemVehicleCondition>, sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(vec![self.make_condition("vehicle_type", "大型")])
    }

    async fn create(
        &self,
        _tenant_id: Uuid,
        item_name: &str,
        _is_required: bool,
        _sort_order: i32,
    ) -> Result<CarryingItem, sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(self.make_item(item_name))
    }

    async fn insert_condition(
        &self,
        _tenant_id: Uuid,
        _item_id: Uuid,
        category: &str,
        value: &str,
    ) -> Result<Option<CarryingItemVehicleCondition>, sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(Some(self.make_condition(category, value)))
    }

    async fn update(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        item_name: Option<&str>,
        _is_required: Option<bool>,
        _sort_order: Option<i32>,
    ) -> Result<Option<CarryingItem>, sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(Some(self.make_item(item_name.unwrap_or("消火器"))))
    }

    async fn delete_conditions(&self, _tenant_id: Uuid, _item_id: Uuid) -> Result<(), sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(())
    }

    async fn get_conditions(
        &self,
        _tenant_id: Uuid,
        _item_id: Uuid,
    ) -> Result<Vec<CarryingItemVehicleCondition>, sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(vec![])
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

async fn spawn_with_mock(mock: Arc<dyn CarryingItemsRepository>) -> (String, String, Uuid) {
    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");

    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.carrying_items = mock;
    let base_url = common::spawn_test_server(state).await;

    (base_url, jwt, tenant_id)
}

fn auth(jwt: &str) -> String {
    format!("Bearer {jwt}")
}

// ============================================================
// list_items: GET /api/carrying-items
// ============================================================

#[tokio::test]
async fn test_list_items_success() {
    test_group!("carrying_items — list");
    test_case!("GET /carrying-items returns 200 with items + conditions", {
        let tenant_id = Uuid::new_v4();
        let mock = Arc::new(SuccessMockCarryingItemsRepository::new(tenant_id));
        let (base_url, jwt, _) = spawn_with_mock(mock).await;
        let client = reqwest::Client::new();

        let res = client
            .get(format!("{base_url}/api/carrying-items"))
            .header("Authorization", auth(&jwt))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);

        let body: Vec<Value> = res.json().await.unwrap();
        assert_eq!(body.len(), 1);
        assert_eq!(body[0]["item_name"], "消火器");
        assert!(body[0]["vehicle_conditions"].is_array());
    });
}

#[tokio::test]
async fn test_list_items_db_error_on_list() {
    test_group!("carrying_items — list (DB error on list)");
    test_case!("GET /carrying-items returns 500 when list fails", {
        let mock = Arc::new(mock_helpers::MockCarryingItemsRepository::default());
        mock.fail_next.store(true, Ordering::SeqCst);
        let (base_url, jwt, _) = spawn_with_mock(mock).await;
        let client = reqwest::Client::new();

        let res = client
            .get(format!("{base_url}/api/carrying-items"))
            .header("Authorization", auth(&jwt))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

#[tokio::test]
async fn test_list_items_db_error_on_conditions() {
    test_group!("carrying_items — list (DB error on list_conditions)");
    test_case!(
        "GET /carrying-items returns 500 when list_conditions fails",
        {
            // Need a mock that succeeds on list() but fails on list_conditions()
            let tenant_id = Uuid::new_v4();
            let mock = Arc::new(SuccessMockCarryingItemsRepository::new(tenant_id));
            // fail_next is consumed by list() — we need a custom approach.
            // Use a wrapper that fails only on list_conditions.
            let mock2 = Arc::new(ListConditionsFailMock);
            let (base_url, jwt, _) = spawn_with_mock(mock2).await;
            let client = reqwest::Client::new();

            let res = client
                .get(format!("{base_url}/api/carrying-items"))
                .header("Authorization", auth(&jwt))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 500);
        }
    );
}

/// Mock that returns items from list() but fails on list_conditions()
struct ListConditionsFailMock;

#[async_trait::async_trait]
impl CarryingItemsRepository for ListConditionsFailMock {
    async fn list(&self, _tenant_id: Uuid) -> Result<Vec<CarryingItem>, sqlx::Error> {
        Ok(vec![CarryingItem {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            item_name: "dummy".to_string(),
            is_required: true,
            sort_order: 0,
            created_at: Utc::now(),
        }])
    }
    async fn list_conditions(
        &self,
        _tenant_id: Uuid,
        _item_ids: &[Uuid],
    ) -> Result<Vec<CarryingItemVehicleCondition>, sqlx::Error> {
        Err(sqlx::Error::RowNotFound)
    }
    async fn create(&self, _: Uuid, _: &str, _: bool, _: i32) -> Result<CarryingItem, sqlx::Error> {
        unreachable!()
    }
    async fn insert_condition(
        &self,
        _: Uuid,
        _: Uuid,
        _: &str,
        _: &str,
    ) -> Result<Option<CarryingItemVehicleCondition>, sqlx::Error> {
        unreachable!()
    }
    async fn update(
        &self,
        _: Uuid,
        _: Uuid,
        _: Option<&str>,
        _: Option<bool>,
        _: Option<i32>,
    ) -> Result<Option<CarryingItem>, sqlx::Error> {
        unreachable!()
    }
    async fn delete_conditions(&self, _: Uuid, _: Uuid) -> Result<(), sqlx::Error> {
        unreachable!()
    }
    async fn get_conditions(
        &self,
        _: Uuid,
        _: Uuid,
    ) -> Result<Vec<CarryingItemVehicleCondition>, sqlx::Error> {
        unreachable!()
    }
    async fn delete(&self, _: Uuid, _: Uuid) -> Result<bool, sqlx::Error> {
        unreachable!()
    }
}

#[tokio::test]
async fn test_list_items_empty() {
    test_group!("carrying_items — list (empty)");
    test_case!(
        "GET /carrying-items returns 200 with empty list (no conditions query)",
        {
            // Default mock returns empty vec — list_conditions is skipped when item_ids is empty
            let mock = Arc::new(mock_helpers::MockCarryingItemsRepository::default());
            let (base_url, jwt, _) = spawn_with_mock(mock).await;
            let client = reqwest::Client::new();

            let res = client
                .get(format!("{base_url}/api/carrying-items"))
                .header("Authorization", auth(&jwt))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Vec<Value> = res.json().await.unwrap();
            assert!(body.is_empty());
        }
    );
}

// ============================================================
// create_item: POST /api/carrying-items
// ============================================================

#[tokio::test]
async fn test_create_item_success() {
    test_group!("carrying_items — create");
    test_case!("POST /carrying-items returns 201 with created item", {
        let tenant_id = Uuid::new_v4();
        let mock = Arc::new(SuccessMockCarryingItemsRepository::new(tenant_id));
        let (base_url, jwt, _) = spawn_with_mock(mock).await;
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/carrying-items"))
            .header("Authorization", auth(&jwt))
            .json(&serde_json::json!({
                "item_name": "三角表示板",
                "is_required": true,
                "sort_order": 1,
                "vehicle_conditions": [
                    { "category": "vehicle_type", "value": "大型" }
                ]
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);

        let body: Value = res.json().await.unwrap();
        assert_eq!(body["item_name"], "三角表示板");
        assert!(body["vehicle_conditions"].is_array());
        assert_eq!(body["vehicle_conditions"].as_array().unwrap().len(), 1);
    });
}

#[tokio::test]
async fn test_create_item_no_conditions() {
    test_group!("carrying_items — create (no conditions)");
    test_case!("POST /carrying-items with empty vehicle_conditions", {
        let tenant_id = Uuid::new_v4();
        let mock = Arc::new(SuccessMockCarryingItemsRepository::new(tenant_id));
        let (base_url, jwt, _) = spawn_with_mock(mock).await;
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/carrying-items"))
            .header("Authorization", auth(&jwt))
            .json(&serde_json::json!({
                "item_name": "発煙筒"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);

        let body: Value = res.json().await.unwrap();
        assert_eq!(body["item_name"], "発煙筒");
        assert_eq!(body["vehicle_conditions"].as_array().unwrap().len(), 0);
    });
}

#[tokio::test]
async fn test_create_item_db_error() {
    test_group!("carrying_items — create (DB error)");
    test_case!("POST /carrying-items returns 500 when create fails", {
        let mock = Arc::new(mock_helpers::MockCarryingItemsRepository::default());
        mock.fail_next.store(true, Ordering::SeqCst);
        let (base_url, jwt, _) = spawn_with_mock(mock).await;
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/carrying-items"))
            .header("Authorization", auth(&jwt))
            .json(&serde_json::json!({
                "item_name": "消火器"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

#[tokio::test]
async fn test_create_item_condition_insert_error() {
    test_group!("carrying_items — create (condition insert error)");
    test_case!(
        "POST /carrying-items returns 500 when insert_condition fails",
        {
            let mock = Arc::new(CreateConditionFailMock);
            let (base_url, jwt, _) = spawn_with_mock(mock).await;
            let client = reqwest::Client::new();

            let res = client
                .post(format!("{base_url}/api/carrying-items"))
                .header("Authorization", auth(&jwt))
                .json(&serde_json::json!({
                    "item_name": "消火器",
                    "vehicle_conditions": [
                        { "category": "vehicle_type", "value": "大型" }
                    ]
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 500);
        }
    );
}

/// Mock that succeeds on create() but fails on insert_condition()
struct CreateConditionFailMock;

#[async_trait::async_trait]
impl CarryingItemsRepository for CreateConditionFailMock {
    async fn list(&self, _: Uuid) -> Result<Vec<CarryingItem>, sqlx::Error> {
        Ok(vec![])
    }
    async fn list_conditions(
        &self,
        _: Uuid,
        _: &[Uuid],
    ) -> Result<Vec<CarryingItemVehicleCondition>, sqlx::Error> {
        Ok(vec![])
    }
    async fn create(
        &self,
        tenant_id: Uuid,
        item_name: &str,
        _: bool,
        _: i32,
    ) -> Result<CarryingItem, sqlx::Error> {
        Ok(CarryingItem {
            id: Uuid::new_v4(),
            tenant_id,
            item_name: item_name.to_string(),
            is_required: true,
            sort_order: 0,
            created_at: Utc::now(),
        })
    }
    async fn insert_condition(
        &self,
        _: Uuid,
        _: Uuid,
        _: &str,
        _: &str,
    ) -> Result<Option<CarryingItemVehicleCondition>, sqlx::Error> {
        Err(sqlx::Error::RowNotFound)
    }
    async fn update(
        &self,
        _: Uuid,
        _: Uuid,
        _: Option<&str>,
        _: Option<bool>,
        _: Option<i32>,
    ) -> Result<Option<CarryingItem>, sqlx::Error> {
        unreachable!()
    }
    async fn delete_conditions(&self, _: Uuid, _: Uuid) -> Result<(), sqlx::Error> {
        unreachable!()
    }
    async fn get_conditions(
        &self,
        _: Uuid,
        _: Uuid,
    ) -> Result<Vec<CarryingItemVehicleCondition>, sqlx::Error> {
        unreachable!()
    }
    async fn delete(&self, _: Uuid, _: Uuid) -> Result<bool, sqlx::Error> {
        unreachable!()
    }
}

// ============================================================
// update_item: PUT /api/carrying-items/{id}
// ============================================================

#[tokio::test]
async fn test_update_item_success_no_conditions() {
    test_group!("carrying_items — update (no vehicle_conditions)");
    test_case!(
        "PUT /carrying-items/{id} returns 200, fetches existing conditions",
        {
            let tenant_id = Uuid::new_v4();
            let mock = Arc::new(SuccessMockCarryingItemsRepository::new(tenant_id));
            let item_id = mock.item_id;
            let (base_url, jwt, _) = spawn_with_mock(mock).await;
            let client = reqwest::Client::new();

            let res = client
                .put(format!("{base_url}/api/carrying-items/{item_id}"))
                .header("Authorization", auth(&jwt))
                .json(&serde_json::json!({
                    "item_name": "更新後の名前"
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);

            let body: Value = res.json().await.unwrap();
            assert_eq!(body["item_name"], "更新後の名前");
            assert!(body["vehicle_conditions"].is_array());
        }
    );
}

#[tokio::test]
async fn test_update_item_success_with_conditions() {
    test_group!("carrying_items — update (with vehicle_conditions replacement)");
    test_case!("PUT /carrying-items/{id} replaces conditions", {
        let tenant_id = Uuid::new_v4();
        let mock = Arc::new(SuccessMockCarryingItemsRepository::new(tenant_id));
        let item_id = mock.item_id;
        let (base_url, jwt, _) = spawn_with_mock(mock).await;
        let client = reqwest::Client::new();

        let res = client
            .put(format!("{base_url}/api/carrying-items/{item_id}"))
            .header("Authorization", auth(&jwt))
            .json(&serde_json::json!({
                "item_name": "更新後",
                "vehicle_conditions": [
                    { "category": "tonnage", "value": "10t" }
                ]
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);

        let body: Value = res.json().await.unwrap();
        assert_eq!(body["vehicle_conditions"].as_array().unwrap().len(), 1);
    });
}

#[tokio::test]
async fn test_update_item_not_found() {
    test_group!("carrying_items — update (not found)");
    test_case!(
        "PUT /carrying-items/{id} returns 404 when item not found",
        {
            // Default mock returns Ok(None) from update
            let mock = Arc::new(mock_helpers::MockCarryingItemsRepository::default());
            let (base_url, jwt, _) = spawn_with_mock(mock).await;
            let client = reqwest::Client::new();
            let fake_id = Uuid::new_v4();

            let res = client
                .put(format!("{base_url}/api/carrying-items/{fake_id}"))
                .header("Authorization", auth(&jwt))
                .json(&serde_json::json!({ "item_name": "xxx" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 404);
        }
    );
}

#[tokio::test]
async fn test_update_item_db_error() {
    test_group!("carrying_items — update (DB error)");
    test_case!("PUT /carrying-items/{id} returns 500 when update fails", {
        let mock = Arc::new(mock_helpers::MockCarryingItemsRepository::default());
        mock.fail_next.store(true, Ordering::SeqCst);
        let (base_url, jwt, _) = spawn_with_mock(mock).await;
        let client = reqwest::Client::new();
        let fake_id = Uuid::new_v4();

        let res = client
            .put(format!("{base_url}/api/carrying-items/{fake_id}"))
            .header("Authorization", auth(&jwt))
            .json(&serde_json::json!({ "item_name": "xxx" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

#[tokio::test]
async fn test_update_item_delete_conditions_error() {
    test_group!("carrying_items — update (delete_conditions error)");
    test_case!(
        "PUT /carrying-items/{id} returns 500 when delete_conditions fails",
        {
            let mock = Arc::new(UpdateDeleteConditionsFailMock);
            let (base_url, jwt, _) = spawn_with_mock(mock).await;
            let client = reqwest::Client::new();
            let fake_id = Uuid::new_v4();

            let res = client
                .put(format!("{base_url}/api/carrying-items/{fake_id}"))
                .header("Authorization", auth(&jwt))
                .json(&serde_json::json!({
                    "item_name": "xxx",
                    "vehicle_conditions": [{ "category": "a", "value": "b" }]
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 500);
        }
    );
}

/// Mock that succeeds on update() but fails on delete_conditions()
struct UpdateDeleteConditionsFailMock;

#[async_trait::async_trait]
impl CarryingItemsRepository for UpdateDeleteConditionsFailMock {
    async fn list(&self, _: Uuid) -> Result<Vec<CarryingItem>, sqlx::Error> {
        Ok(vec![])
    }
    async fn list_conditions(
        &self,
        _: Uuid,
        _: &[Uuid],
    ) -> Result<Vec<CarryingItemVehicleCondition>, sqlx::Error> {
        Ok(vec![])
    }
    async fn create(&self, _: Uuid, _: &str, _: bool, _: i32) -> Result<CarryingItem, sqlx::Error> {
        unreachable!()
    }
    async fn insert_condition(
        &self,
        _: Uuid,
        _: Uuid,
        _: &str,
        _: &str,
    ) -> Result<Option<CarryingItemVehicleCondition>, sqlx::Error> {
        unreachable!()
    }
    async fn update(
        &self,
        _: Uuid,
        _: Uuid,
        _: Option<&str>,
        _: Option<bool>,
        _: Option<i32>,
    ) -> Result<Option<CarryingItem>, sqlx::Error> {
        Ok(Some(CarryingItem {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            item_name: "dummy".to_string(),
            is_required: true,
            sort_order: 0,
            created_at: Utc::now(),
        }))
    }
    async fn delete_conditions(&self, _: Uuid, _: Uuid) -> Result<(), sqlx::Error> {
        Err(sqlx::Error::RowNotFound)
    }
    async fn get_conditions(
        &self,
        _: Uuid,
        _: Uuid,
    ) -> Result<Vec<CarryingItemVehicleCondition>, sqlx::Error> {
        Ok(vec![])
    }
    async fn delete(&self, _: Uuid, _: Uuid) -> Result<bool, sqlx::Error> {
        unreachable!()
    }
}

#[tokio::test]
async fn test_update_item_insert_condition_error() {
    test_group!("carrying_items — update (insert_condition error after delete)");
    test_case!(
        "PUT /carrying-items/{id} returns 500 when re-insert condition fails",
        {
            let mock = Arc::new(UpdateInsertConditionFailMock);
            let (base_url, jwt, _) = spawn_with_mock(mock).await;
            let client = reqwest::Client::new();
            let fake_id = Uuid::new_v4();

            let res = client
                .put(format!("{base_url}/api/carrying-items/{fake_id}"))
                .header("Authorization", auth(&jwt))
                .json(&serde_json::json!({
                    "item_name": "xxx",
                    "vehicle_conditions": [{ "category": "a", "value": "b" }]
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 500);
        }
    );
}

/// Mock that succeeds on update + delete_conditions but fails on insert_condition
struct UpdateInsertConditionFailMock;

#[async_trait::async_trait]
impl CarryingItemsRepository for UpdateInsertConditionFailMock {
    async fn list(&self, _: Uuid) -> Result<Vec<CarryingItem>, sqlx::Error> {
        Ok(vec![])
    }
    async fn list_conditions(
        &self,
        _: Uuid,
        _: &[Uuid],
    ) -> Result<Vec<CarryingItemVehicleCondition>, sqlx::Error> {
        Ok(vec![])
    }
    async fn create(&self, _: Uuid, _: &str, _: bool, _: i32) -> Result<CarryingItem, sqlx::Error> {
        unreachable!()
    }
    async fn insert_condition(
        &self,
        _: Uuid,
        _: Uuid,
        _: &str,
        _: &str,
    ) -> Result<Option<CarryingItemVehicleCondition>, sqlx::Error> {
        Err(sqlx::Error::RowNotFound)
    }
    async fn update(
        &self,
        _: Uuid,
        _: Uuid,
        _: Option<&str>,
        _: Option<bool>,
        _: Option<i32>,
    ) -> Result<Option<CarryingItem>, sqlx::Error> {
        Ok(Some(CarryingItem {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            item_name: "dummy".to_string(),
            is_required: true,
            sort_order: 0,
            created_at: Utc::now(),
        }))
    }
    async fn delete_conditions(&self, _: Uuid, _: Uuid) -> Result<(), sqlx::Error> {
        Ok(())
    }
    async fn get_conditions(
        &self,
        _: Uuid,
        _: Uuid,
    ) -> Result<Vec<CarryingItemVehicleCondition>, sqlx::Error> {
        Ok(vec![])
    }
    async fn delete(&self, _: Uuid, _: Uuid) -> Result<bool, sqlx::Error> {
        unreachable!()
    }
}

#[tokio::test]
async fn test_update_item_get_conditions_error() {
    test_group!("carrying_items — update (get_conditions error)");
    test_case!("PUT /carrying-items/{id} returns 500 when get_conditions fails (no vehicle_conditions in body)", {
        let mock = Arc::new(UpdateGetConditionsFailMock);
        let (base_url, jwt, _) = spawn_with_mock(mock).await;
        let client = reqwest::Client::new();
        let fake_id = Uuid::new_v4();

        // No vehicle_conditions in body => takes the else branch which calls get_conditions
        let res = client
            .put(format!("{base_url}/api/carrying-items/{fake_id}"))
            .header("Authorization", auth(&jwt))
            .json(&serde_json::json!({ "item_name": "xxx" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

/// Mock that succeeds on update() but fails on get_conditions()
struct UpdateGetConditionsFailMock;

#[async_trait::async_trait]
impl CarryingItemsRepository for UpdateGetConditionsFailMock {
    async fn list(&self, _: Uuid) -> Result<Vec<CarryingItem>, sqlx::Error> {
        Ok(vec![])
    }
    async fn list_conditions(
        &self,
        _: Uuid,
        _: &[Uuid],
    ) -> Result<Vec<CarryingItemVehicleCondition>, sqlx::Error> {
        Ok(vec![])
    }
    async fn create(&self, _: Uuid, _: &str, _: bool, _: i32) -> Result<CarryingItem, sqlx::Error> {
        unreachable!()
    }
    async fn insert_condition(
        &self,
        _: Uuid,
        _: Uuid,
        _: &str,
        _: &str,
    ) -> Result<Option<CarryingItemVehicleCondition>, sqlx::Error> {
        unreachable!()
    }
    async fn update(
        &self,
        _: Uuid,
        _: Uuid,
        _: Option<&str>,
        _: Option<bool>,
        _: Option<i32>,
    ) -> Result<Option<CarryingItem>, sqlx::Error> {
        Ok(Some(CarryingItem {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            item_name: "dummy".to_string(),
            is_required: true,
            sort_order: 0,
            created_at: Utc::now(),
        }))
    }
    async fn delete_conditions(&self, _: Uuid, _: Uuid) -> Result<(), sqlx::Error> {
        unreachable!()
    }
    async fn get_conditions(
        &self,
        _: Uuid,
        _: Uuid,
    ) -> Result<Vec<CarryingItemVehicleCondition>, sqlx::Error> {
        Err(sqlx::Error::RowNotFound)
    }
    async fn delete(&self, _: Uuid, _: Uuid) -> Result<bool, sqlx::Error> {
        unreachable!()
    }
}

// ============================================================
// delete_item: DELETE /api/carrying-items/{id}
// ============================================================

#[tokio::test]
async fn test_delete_item_success() {
    test_group!("carrying_items — delete");
    test_case!("DELETE /carrying-items/{id} returns 204", {
        let tenant_id = Uuid::new_v4();
        let mock = Arc::new(SuccessMockCarryingItemsRepository::new(tenant_id));
        let item_id = mock.item_id;
        let (base_url, jwt, _) = spawn_with_mock(mock).await;
        let client = reqwest::Client::new();

        let res = client
            .delete(format!("{base_url}/api/carrying-items/{item_id}"))
            .header("Authorization", auth(&jwt))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 204);
    });
}

#[tokio::test]
async fn test_delete_item_not_found() {
    test_group!("carrying_items — delete (not found)");
    test_case!(
        "DELETE /carrying-items/{id} returns 404 when not deleted",
        {
            // Default mock returns Ok(false) from delete
            let mock = Arc::new(mock_helpers::MockCarryingItemsRepository::default());
            let (base_url, jwt, _) = spawn_with_mock(mock).await;
            let client = reqwest::Client::new();
            let fake_id = Uuid::new_v4();

            let res = client
                .delete(format!("{base_url}/api/carrying-items/{fake_id}"))
                .header("Authorization", auth(&jwt))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 404);
        }
    );
}

#[tokio::test]
async fn test_delete_item_db_error() {
    test_group!("carrying_items — delete (DB error)");
    test_case!(
        "DELETE /carrying-items/{id} returns 500 when delete fails",
        {
            let mock = Arc::new(mock_helpers::MockCarryingItemsRepository::default());
            mock.fail_next.store(true, Ordering::SeqCst);
            let (base_url, jwt, _) = spawn_with_mock(mock).await;
            let client = reqwest::Client::new();
            let fake_id = Uuid::new_v4();

            let res = client
                .delete(format!("{base_url}/api/carrying-items/{fake_id}"))
                .header("Authorization", auth(&jwt))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 500);
        }
    );
}

// ============================================================
// Auth: no JWT → 401
// ============================================================

#[tokio::test]
async fn test_no_auth_returns_401() {
    test_group!("carrying_items — auth");
    test_case!("All endpoints return 401 without JWT", {
        let mock = Arc::new(mock_helpers::MockCarryingItemsRepository::default());
        let mut state = mock_helpers::app_state::setup_mock_app_state();
        state.carrying_items = mock;
        let base_url = common::spawn_test_server(state).await;
        let client = reqwest::Client::new();
        let fake_id = Uuid::new_v4();

        // GET
        let res = client
            .get(format!("{base_url}/api/carrying-items"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 401);

        // POST
        let res = client
            .post(format!("{base_url}/api/carrying-items"))
            .json(&serde_json::json!({ "item_name": "x" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 401);

        // PUT
        let res = client
            .put(format!("{base_url}/api/carrying-items/{fake_id}"))
            .json(&serde_json::json!({ "item_name": "x" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 401);

        // DELETE
        let res = client
            .delete(format!("{base_url}/api/carrying-items/{fake_id}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 401);
    });
}

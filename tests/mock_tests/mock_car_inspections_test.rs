use uuid::Uuid;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::Value;

use rust_alc_api::db::repository::car_inspections::{
    CarInspectionFile, CarInspectionRepository, VehicleCategories,
};

// ============================================================
// SuccessMock: returns realistic data for success-path tests
// ============================================================

struct SuccessMockCarInspectionRepository {
    fail_next: AtomicBool,
}

impl SuccessMockCarInspectionRepository {
    fn new() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl CarInspectionRepository for SuccessMockCarInspectionRepository {
    async fn list_current(&self, _tenant_id: Uuid) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(vec![serde_json::json!({"id": 1, "CarId": "ABC-123"})])
    }

    async fn list_expired(&self, _tenant_id: Uuid) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(vec![
            serde_json::json!({"id": 2, "CarId": "DEF-456", "expired": true}),
        ])
    }

    async fn list_renew(&self, _tenant_id: Uuid) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(vec![
            serde_json::json!({"id": 3, "CarId": "GHI-789", "renew": true}),
        ])
    }

    async fn get_by_id(
        &self,
        _tenant_id: Uuid,
        id: i32,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        if id == 999 {
            return Ok(None);
        }
        Ok(Some(serde_json::json!({"id": id, "CarId": "ABC-123"})))
    }

    async fn vehicle_categories(&self, _tenant_id: Uuid) -> Result<VehicleCategories, sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(VehicleCategories {
            car_kinds: vec!["普通".to_string(), "小型".to_string()],
            uses: vec!["自家用".to_string()],
            car_shapes: vec!["箱型".to_string()],
            private_businesses: vec!["運送".to_string()],
        })
    }

    async fn list_current_files(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<CarInspectionFile>, sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(vec![])
    }
}

// ============================================================
// Helper: spawn server with a given mock
// ============================================================

async fn spawn_with_mock(mock: Arc<dyn CarInspectionRepository>) -> (String, String) {
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.car_inspections = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    (base_url, jwt)
}

fn auth(jwt: &str) -> String {
    format!("Bearer {jwt}")
}

// ============================================================
// list_current: GET /api/car-inspections/current
// ============================================================

#[tokio::test]
async fn test_list_current_success() {
    let mock = Arc::new(SuccessMockCarInspectionRepository::new());
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/car-inspections/current"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    let inspections = body["carInspections"].as_array().unwrap();
    assert_eq!(inspections.len(), 1);
    assert_eq!(inspections[0]["CarId"], "ABC-123");
}

#[tokio::test]
async fn test_list_current_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockCarInspectionRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/car-inspections/current"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// list_expired: GET /api/car-inspections/expired
// ============================================================

#[tokio::test]
async fn test_list_expired_success() {
    let mock = Arc::new(SuccessMockCarInspectionRepository::new());
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/car-inspections/expired"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    let inspections = body["carInspections"].as_array().unwrap();
    assert_eq!(inspections.len(), 1);
    assert_eq!(inspections[0]["CarId"], "DEF-456");
}

#[tokio::test]
async fn test_list_expired_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockCarInspectionRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/car-inspections/expired"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// list_renew: GET /api/car-inspections/renew
// ============================================================

#[tokio::test]
async fn test_list_renew_success() {
    let mock = Arc::new(SuccessMockCarInspectionRepository::new());
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/car-inspections/renew"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    let inspections = body["carInspections"].as_array().unwrap();
    assert_eq!(inspections.len(), 1);
    assert_eq!(inspections[0]["CarId"], "GHI-789");
}

#[tokio::test]
async fn test_list_renew_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockCarInspectionRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/car-inspections/renew"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// vehicle_categories: GET /api/car-inspections/vehicle-categories
// ============================================================

#[tokio::test]
async fn test_vehicle_categories_success() {
    let mock = Arc::new(SuccessMockCarInspectionRepository::new());
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/car-inspections/vehicle-categories"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    let car_kinds = body["car_kinds"].as_array().unwrap();
    assert_eq!(car_kinds.len(), 2);
    assert_eq!(car_kinds[0], "普通");
    assert_eq!(car_kinds[1], "小型");

    let uses = body["uses"].as_array().unwrap();
    assert_eq!(uses.len(), 1);
    assert_eq!(uses[0], "自家用");

    let car_shapes = body["car_shapes"].as_array().unwrap();
    assert_eq!(car_shapes.len(), 1);

    let private_businesses = body["private_businesses"].as_array().unwrap();
    assert_eq!(private_businesses.len(), 1);
}

#[tokio::test]
async fn test_vehicle_categories_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockCarInspectionRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/car-inspections/vehicle-categories"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// get_by_id: GET /api/car-inspections/{id}
// ============================================================

#[tokio::test]
async fn test_get_by_id_success() {
    let mock = Arc::new(SuccessMockCarInspectionRepository::new());
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/car-inspections/42"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    assert_eq!(body["id"], 42);
    assert_eq!(body["CarId"], "ABC-123");
}

#[tokio::test]
async fn test_get_by_id_not_found() {
    let mock = Arc::new(SuccessMockCarInspectionRepository::new());
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    // id=999 returns None in our mock
    let res = client
        .get(format!("{base_url}/api/car-inspections/999"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_get_by_id_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockCarInspectionRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/car-inspections/1"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

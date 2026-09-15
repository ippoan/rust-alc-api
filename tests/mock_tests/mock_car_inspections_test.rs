use uuid::Uuid;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::Value;

use rust_alc_api::db::repository::car_inspections::{
    CarInspectionFile, CarInspectionRepository, CarinsLookup, VehicleCategories,
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

    async fn list_by_car_id(
        &self,
        _tenant_id: Uuid,
        car_id: &str,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        if car_id == "NONEXISTENT" {
            return Ok(vec![]);
        }
        Ok(vec![
            serde_json::json!({
                "id": 10,
                "CarId": car_id,
                "TwodimensionCodeInfoValidPeriodExpirdate": "270401",
                "pdfUuid": "11111111-1111-1111-1111-111111111111",
                "jsonUuid": null,
            }),
            serde_json::json!({
                "id": 9,
                "CarId": car_id,
                "TwodimensionCodeInfoValidPeriodExpirdate": "250401",
                "pdfUuid": null,
                "jsonUuid": "22222222-2222-2222-2222-222222222222",
            }),
        ])
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

    async fn upsert_from_json(
        &self,
        _tenant_id: Uuid,
        _cert_info: &serde_json::Value,
        _cert_info_import_file_version: &str,
    ) -> Result<(), sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(())
    }

    async fn create_file_link(
        &self,
        _params: &alc_core::repository::car_inspections::CreateFileLinkParams<'_>,
    ) -> Result<(), sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(())
    }

    async fn find_pending_pdf(
        &self,
        _tenant_id: Uuid,
        _elect_cert_mg_no: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(None)
    }

    async fn delete_pending_pdf(
        &self,
        _tenant_id: Uuid,
        _elect_cert_mg_no: &str,
    ) -> Result<(), sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(())
    }

    async fn upsert_pending_pdf(
        &self,
        _params: &alc_core::repository::car_inspections::CreateFileLinkParams<'_>,
    ) -> Result<(), sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(())
    }

    async fn json_file_exists(
        &self,
        _tenant_id: Uuid,
        _elect_cert_mg_no: &str,
        _grantdate_e: &str,
        _grantdate_y: &str,
        _grantdate_m: &str,
        _grantdate_d: &str,
    ) -> Result<bool, sqlx::Error> {
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(false)
    }

    async fn lookup_expiry(
        &self,
        _tenant_id: Uuid,
        cert_no: Option<&str>,
        car_id: Option<&str>,
    ) -> Result<CarinsLookup, sqlx::Error> {
        let expires_on = chrono::NaiveDate::from_ymd_opt(2030, 12, 31);
        let car_no = Some("TEST-CAR-NO".to_string());
        Ok(match (cert_no, car_id) {
            (Some("000000000001"), _) => CarinsLookup {
                expires_on,
                matched_by: "cert_no",
                car_no,
            },
            (_, Some("TESTCARID00001")) => CarinsLookup {
                expires_on,
                matched_by: "car_id",
                car_no,
            },
            _ => CarinsLookup {
                expires_on: None,
                matched_by: "none",
                car_no: None,
            },
        })
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
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

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

// ============================================================
// list_history: GET /api/car-inspections/by-car/{car_id}/history
// ============================================================

#[tokio::test]
async fn test_list_history_success() {
    let mock = Arc::new(SuccessMockCarInspectionRepository::new());
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!(
            "{base_url}/api/car-inspections/by-car/ABC-123/history"
        ))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    let inspections = body["carInspections"].as_array().unwrap();
    assert_eq!(inspections.len(), 2);
    // 新→旧の順
    assert_eq!(inspections[0]["id"], 10);
    assert_eq!(
        inspections[0]["TwodimensionCodeInfoValidPeriodExpirdate"],
        "270401"
    );
    assert_eq!(inspections[1]["id"], 9);
    // pdfUuid/jsonUuid が含まれる
    assert_eq!(
        inspections[0]["pdfUuid"],
        "11111111-1111-1111-1111-111111111111"
    );
    assert_eq!(
        inspections[1]["jsonUuid"],
        "22222222-2222-2222-2222-222222222222"
    );
}

#[tokio::test]
async fn test_list_history_empty() {
    let mock = Arc::new(SuccessMockCarInspectionRepository::new());
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!(
            "{base_url}/api/car-inspections/by-car/NONEXISTENT/history"
        ))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    let inspections = body["carInspections"].as_array().unwrap();
    assert!(inspections.is_empty());
}

#[tokio::test]
async fn test_list_history_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockCarInspectionRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!(
            "{base_url}/api/car-inspections/by-car/ABC-123/history"
        ))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_list_history_unauthorized() {
    let mock = Arc::new(SuccessMockCarInspectionRepository::new());
    let (base_url, _jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    // JWT なし → 401 (テナント未解決)
    let res = client
        .get(format!(
            "{base_url}/api/car-inspections/by-car/ABC-123/history"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

// ============================================================
// lookup: POST /api/car-inspections/lookup (kiosk の車検期限の照合)
// ============================================================

async fn post_lookup(
    mock: Arc<dyn CarInspectionRepository>,
    body: serde_json::Value,
) -> reqwest::Response {
    let (base_url, jwt) = spawn_with_mock(mock).await;
    reqwest::Client::new()
        .post(format!("{base_url}/api/car-inspections/lookup"))
        .header("Authorization", auth(&jwt))
        .json(&body)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn test_lookup_matched_by_cert_no() {
    let mock = Arc::new(SuccessMockCarInspectionRepository::new());
    let res = post_lookup(
        mock,
        serde_json::json!({"cert_no": "000000000001", "car_id": "TESTCARID00009"}),
    )
    .await;
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(
        body,
        serde_json::json!({
            "expires_on": "2030-12-31",
            "matched_by": "cert_no",
            "car_no": "TEST-CAR-NO",
        }),
        "期限・matched_by・登録番号だけを返す"
    );
}

#[tokio::test]
async fn test_lookup_matched_by_car_id() {
    let mock = Arc::new(SuccessMockCarInspectionRepository::new());
    let res = post_lookup(mock, serde_json::json!({"car_id": "TESTCARID00001"})).await;
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["matched_by"], "car_id");
    assert_eq!(body["expires_on"], "2030-12-31");
}

#[tokio::test]
async fn test_lookup_none() {
    let mock = Arc::new(SuccessMockCarInspectionRepository::new());
    let res = post_lookup(mock, serde_json::json!({"cert_no": "000000000002"})).await;
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(
        body,
        serde_json::json!({"expires_on": null, "matched_by": "none", "car_no": null})
    );
}

#[tokio::test]
async fn test_lookup_without_numbers_is_bad_request() {
    // repository まで届いたら 500 になる = 400 は repository より前で返っている
    let mock = Arc::new(crate::mock_helpers::MockCarInspectionRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let res = post_lookup(mock, serde_json::json!({"cert_no": "", "car_id": null})).await;
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_lookup_bad_shape_is_bad_request() {
    let mock = Arc::new(crate::mock_helpers::MockCarInspectionRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let res = post_lookup(mock, serde_json::json!({"cert_no": "12345"})).await;
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_lookup_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockCarInspectionRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let res = post_lookup(mock, serde_json::json!({"car_id": "TESTCARID00001"})).await;
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn test_lookup_get_is_not_routed_to_get_by_id() {
    // GET では番号を URL に載せない — POST 専用。`/{id}` の GET に吸われない
    let mock = Arc::new(SuccessMockCarInspectionRepository::new());
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let res = reqwest::Client::new()
        .get(format!("{base_url}/api/car-inspections/lookup"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 405);
}

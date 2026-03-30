mod common;
mod mock_helpers;

use uuid::Uuid;

use std::sync::Arc;

use chrono::Utc;
use mock_helpers::MockHealthBaselinesRepository;
use rust_alc_api::db::models::EmployeeHealthBaseline;

// ---------------------------------------------------------------------------
// Helper: spawn server with default mock state
// ---------------------------------------------------------------------------

async fn setup() -> (String, String) {
    let state = mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let base = common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

/// spawn server with a custom health_baselines mock
async fn setup_with_mock(mock: Arc<MockHealthBaselinesRepository>) -> (String, String) {
    let mut state = mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
    state.health_baselines = mock;
    let base = common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

/// spawn server with fail_next=true on health_baselines mock
async fn setup_failing() -> (String, String) {
    let mock = Arc::new(MockHealthBaselinesRepository::default());
    mock.fail_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = common::create_test_jwt(tenant_id, "admin");
    state.health_baselines = mock;
    let base = common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

fn make_sample_baseline(tenant_id: uuid::Uuid, employee_id: uuid::Uuid) -> EmployeeHealthBaseline {
    EmployeeHealthBaseline {
        id: uuid::Uuid::new_v4(),
        tenant_id,
        employee_id,
        baseline_systolic: 120,
        baseline_diastolic: 80,
        baseline_temperature: 36.5,
        systolic_tolerance: 10,
        diastolic_tolerance: 10,
        temperature_tolerance: 0.5,
        measurement_validity_minutes: 30,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

// ===========================================================================
// POST /api/tenko/health-baselines — upsert_baseline
// ===========================================================================

#[tokio::test]
async fn upsert_baseline_success() {
    let (base, auth) = setup().await;
    let employee_id = uuid::Uuid::new_v4();
    let res = client()
        .post(format!("{base}/api/tenko/health-baselines"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "employee_id": employee_id.to_string(),
            "baseline_systolic": 130,
            "baseline_diastolic": 85
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["employee_id"], employee_id.to_string());
    assert_eq!(body["baseline_systolic"], 130);
    assert_eq!(body["baseline_diastolic"], 85);
}

#[tokio::test]
async fn upsert_baseline_with_defaults() {
    let (base, auth) = setup().await;
    let employee_id = uuid::Uuid::new_v4();
    let res = client()
        .post(format!("{base}/api/tenko/health-baselines"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "employee_id": employee_id.to_string()
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["employee_id"], employee_id.to_string());
    // defaults
    assert_eq!(body["baseline_systolic"], 120);
    assert_eq!(body["baseline_diastolic"], 80);
    assert_eq!(body["baseline_temperature"], 36.5);
    assert_eq!(body["systolic_tolerance"], 10);
    assert_eq!(body["diastolic_tolerance"], 10);
    assert_eq!(body["temperature_tolerance"], 0.5);
    assert_eq!(body["measurement_validity_minutes"], 30);
}

#[tokio::test]
async fn upsert_baseline_with_all_fields() {
    let (base, auth) = setup().await;
    let employee_id = uuid::Uuid::new_v4();
    let res = client()
        .post(format!("{base}/api/tenko/health-baselines"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "employee_id": employee_id.to_string(),
            "baseline_systolic": 140,
            "baseline_diastolic": 90,
            "baseline_temperature": 37.0,
            "systolic_tolerance": 15,
            "diastolic_tolerance": 12,
            "temperature_tolerance": 0.8,
            "measurement_validity_minutes": 60
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["baseline_systolic"], 140);
    assert_eq!(body["baseline_diastolic"], 90);
    assert_eq!(body["baseline_temperature"], 37.0);
    assert_eq!(body["systolic_tolerance"], 15);
    assert_eq!(body["diastolic_tolerance"], 12);
    assert_eq!(body["temperature_tolerance"], 0.8);
    assert_eq!(body["measurement_validity_minutes"], 60);
}

#[tokio::test]
async fn upsert_baseline_db_error_returns_500() {
    let (base, auth) = setup_failing().await;
    let res = client()
        .post(format!("{base}/api/tenko/health-baselines"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "employee_id": uuid::Uuid::new_v4().to_string()
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn upsert_baseline_no_auth_returns_401() {
    let (base, _auth) = setup().await;
    let res = client()
        .post(format!("{base}/api/tenko/health-baselines"))
        .json(&serde_json::json!({
            "employee_id": uuid::Uuid::new_v4().to_string()
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

// ===========================================================================
// GET /api/tenko/health-baselines — list_baselines
// ===========================================================================

#[tokio::test]
async fn list_baselines_success_empty() {
    let (base, auth) = setup().await;
    let res = client()
        .get(format!("{base}/api/tenko/health-baselines"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Vec<serde_json::Value> = res.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn list_baselines_success_with_data() {
    let tenant_id = uuid::Uuid::new_v4();
    let employee_id = uuid::Uuid::new_v4();
    let baseline = make_sample_baseline(tenant_id, employee_id);
    let baseline_id = baseline.id;

    let mock = Arc::new(MockHealthBaselinesRepository::default());
    *mock.list_result.lock().unwrap() = vec![baseline];
    let (base, auth) = setup_with_mock(mock).await;

    let res = client()
        .get(format!("{base}/api/tenko/health-baselines"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Vec<serde_json::Value> = res.json().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["id"], baseline_id.to_string());
    assert_eq!(body[0]["employee_id"], employee_id.to_string());
}

#[tokio::test]
async fn list_baselines_db_error_returns_500() {
    let (base, auth) = setup_failing().await;
    let res = client()
        .get(format!("{base}/api/tenko/health-baselines"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn list_baselines_no_auth_returns_401() {
    let (base, _auth) = setup().await;
    let res = client()
        .get(format!("{base}/api/tenko/health-baselines"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

// ===========================================================================
// GET /api/tenko/health-baselines/{employee_id} — get_baseline
// ===========================================================================

#[tokio::test]
async fn get_baseline_not_found() {
    let (base, auth) = setup().await;
    let employee_id = uuid::Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/tenko/health-baselines/{employee_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn get_baseline_found() {
    let tenant_id = uuid::Uuid::new_v4();
    let employee_id = uuid::Uuid::new_v4();
    let baseline = make_sample_baseline(tenant_id, employee_id);

    let mock = Arc::new(MockHealthBaselinesRepository::default());
    *mock.get_result.lock().unwrap() = Some(baseline);
    let (base, auth) = setup_with_mock(mock).await;

    let res = client()
        .get(format!("{base}/api/tenko/health-baselines/{employee_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["employee_id"], employee_id.to_string());
    assert_eq!(body["baseline_systolic"], 120);
}

#[tokio::test]
async fn get_baseline_db_error_returns_500() {
    let (base, auth) = setup_failing().await;
    let employee_id = uuid::Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/tenko/health-baselines/{employee_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn get_baseline_no_auth_returns_401() {
    let (base, _auth) = setup().await;
    let employee_id = uuid::Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/tenko/health-baselines/{employee_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

// ===========================================================================
// PUT /api/tenko/health-baselines/{employee_id} — update_baseline
// ===========================================================================

#[tokio::test]
async fn update_baseline_not_found() {
    let (base, auth) = setup().await;
    let employee_id = uuid::Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/tenko/health-baselines/{employee_id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "baseline_systolic": 130
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn update_baseline_found() {
    let tenant_id = uuid::Uuid::new_v4();
    let employee_id = uuid::Uuid::new_v4();
    let mut baseline = make_sample_baseline(tenant_id, employee_id);
    baseline.baseline_systolic = 135;

    let mock = Arc::new(MockHealthBaselinesRepository::default());
    *mock.update_result.lock().unwrap() = Some(baseline);
    let (base, auth) = setup_with_mock(mock).await;

    let res = client()
        .put(format!("{base}/api/tenko/health-baselines/{employee_id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "baseline_systolic": 135
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["employee_id"], employee_id.to_string());
    assert_eq!(body["baseline_systolic"], 135);
}

#[tokio::test]
async fn update_baseline_db_error_returns_500() {
    let (base, auth) = setup_failing().await;
    let employee_id = uuid::Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/tenko/health-baselines/{employee_id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "baseline_systolic": 130
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn update_baseline_no_auth_returns_401() {
    let (base, _auth) = setup().await;
    let employee_id = uuid::Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/tenko/health-baselines/{employee_id}"))
        .json(&serde_json::json!({
            "baseline_systolic": 130
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

// ===========================================================================
// DELETE /api/tenko/health-baselines/{employee_id} — delete_baseline
// ===========================================================================

#[tokio::test]
async fn delete_baseline_not_found() {
    let (base, auth) = setup().await;
    let employee_id = uuid::Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/tenko/health-baselines/{employee_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    // Mock returns false => 404
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn delete_baseline_success() {
    let mock = Arc::new(MockHealthBaselinesRepository::default());
    *mock.delete_result.lock().unwrap() = true;
    let (base, auth) = setup_with_mock(mock).await;

    let employee_id = uuid::Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/tenko/health-baselines/{employee_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn delete_baseline_db_error_returns_500() {
    let (base, auth) = setup_failing().await;
    let employee_id = uuid::Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/tenko/health-baselines/{employee_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn delete_baseline_no_auth_returns_401() {
    let (base, _auth) = setup().await;
    let employee_id = uuid::Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/tenko/health-baselines/{employee_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

// ===========================================================================
// X-Tenant-ID header fallback (no JWT)
// ===========================================================================

#[tokio::test]
async fn list_baselines_with_tenant_header() {
    let state = mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base = common::spawn_test_server(state).await;

    let res = client()
        .get(format!("{base}/api/tenko/health-baselines"))
        .header("X-Tenant-ID", tenant_id.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Vec<serde_json::Value> = res.json().await.unwrap();
    assert!(body.is_empty());
}

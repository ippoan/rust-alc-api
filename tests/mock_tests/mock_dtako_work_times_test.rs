use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::mock_helpers::app_state::setup_mock_app_state;
use crate::mock_helpers::MockDtakoWorkTimesRepository;

// ---------------------------------------------------------------------------
// Helper: build mock AppState and return a handle to the work_times mock
// ---------------------------------------------------------------------------
async fn setup() -> (rust_alc_api::AppState, Arc<MockDtakoWorkTimesRepository>) {
    let mut state = setup_mock_app_state();
    let mock_wt = Arc::new(MockDtakoWorkTimesRepository::default());
    state.dtako_work_times = mock_wt.clone();
    (state, mock_wt)
}

// ---------------------------------------------------------------------------
// GET /api/work-times — success (empty)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn mock_list_work_times_success_empty() {
    let (state, _mock_wt) = setup().await;
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/work-times"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["items"], serde_json::json!([]));
    assert_eq!(body["total"], 0);
    assert_eq!(body["page"], 1);
    assert_eq!(body["per_page"], 50);
}

// ---------------------------------------------------------------------------
// GET /api/work-times — with query params (page, per_page, driver_id, dates)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn mock_list_work_times_with_query_params() {
    let (state, _mock_wt) = setup().await;
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let driver_id = uuid::Uuid::new_v4();
    let res = client
        .get(format!(
            "{base_url}/api/work-times?page=2&per_page=10&driver_id={driver_id}&date_from=2026-01-01&date_to=2026-12-31"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["items"], serde_json::json!([]));
    assert_eq!(body["total"], 0);
    assert_eq!(body["page"], 2);
    assert_eq!(body["per_page"], 10);
}

// ---------------------------------------------------------------------------
// GET /api/work-times — per_page clamped to 200
// ---------------------------------------------------------------------------
#[tokio::test]
async fn mock_list_work_times_per_page_max() {
    let (state, _mock_wt) = setup().await;
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/work-times?per_page=999"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    // per_page should be clamped to 200
    assert_eq!(body["per_page"], 200);
}

// ---------------------------------------------------------------------------
// GET /api/work-times — page < 1 defaults to 1
// ---------------------------------------------------------------------------
#[tokio::test]
async fn mock_list_work_times_page_min() {
    let (state, _mock_wt) = setup().await;
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/work-times?page=0"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    // page=0 should be clamped to 1
    assert_eq!(body["page"], 1);
}

// ---------------------------------------------------------------------------
// GET /api/work-times — no auth → 401
// ---------------------------------------------------------------------------
#[tokio::test]
async fn mock_list_work_times_no_auth() {
    let (state, _mock_wt) = setup().await;
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/work-times"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

// ---------------------------------------------------------------------------
// GET /api/work-times — X-Tenant-ID header (kiosk mode)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn mock_list_work_times_x_tenant_id() {
    let (state, _mock_wt) = setup().await;
    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/work-times"))
        .header("X-Tenant-ID", tenant_id.to_string())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["total"], 0);
}

// ---------------------------------------------------------------------------
// GET /api/work-times — DB error on count → 500
// ---------------------------------------------------------------------------
#[tokio::test]
async fn mock_list_work_times_count_db_error() {
    let (state, mock_wt) = setup().await;
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    // fail_next will trigger on the first repo call (count)
    mock_wt.fail_next.store(true, Ordering::SeqCst);

    let res = client
        .get(format!("{base_url}/api/work-times"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// ---------------------------------------------------------------------------
// GET /api/work-times — DB error on list (count succeeds, list fails) → 500
// ---------------------------------------------------------------------------
#[tokio::test]
async fn mock_list_work_times_list_db_error() {
    let (state, mock_wt) = setup().await;
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");

    // We need count to succeed and list to fail.
    // Since check_fail! uses swap(false), the first call consumes the flag.
    // We need a custom mock for this scenario.
    // Instead, use a specialized mock that fails only on list.
    let fail_on_list_mock = Arc::new(MockDtakoWorkTimesFailList::default());
    let mut state_clone = state;
    state_clone.dtako_work_times = fail_on_list_mock.clone();

    let base_url = crate::common::spawn_test_server(state_clone).await;
    let client = reqwest::Client::new();

    fail_on_list_mock.fail_list.store(true, Ordering::SeqCst);

    let res = client
        .get(format!("{base_url}/api/work-times"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// ---------------------------------------------------------------------------
// Specialized mock: count succeeds, list fails
// ---------------------------------------------------------------------------
use rust_alc_api::db::repository::dtako_work_times::{DtakoWorkTimesRepository, WorkTimeItem};
use std::sync::atomic::AtomicBool;

pub struct MockDtakoWorkTimesFailList {
    pub fail_list: AtomicBool,
}

impl Default for MockDtakoWorkTimesFailList {
    fn default() -> Self {
        Self {
            fail_list: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl DtakoWorkTimesRepository for MockDtakoWorkTimesFailList {
    async fn count(
        &self,
        _tenant_id: uuid::Uuid,
        _driver_id: Option<uuid::Uuid>,
        _date_from: Option<chrono::NaiveDate>,
        _date_to: Option<chrono::NaiveDate>,
    ) -> Result<i64, sqlx::Error> {
        // count always succeeds
        Ok(0)
    }

    async fn list(
        &self,
        _tenant_id: uuid::Uuid,
        _driver_id: Option<uuid::Uuid>,
        _date_from: Option<chrono::NaiveDate>,
        _date_to: Option<chrono::NaiveDate>,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<WorkTimeItem>, sqlx::Error> {
        if self.fail_list.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(vec![])
    }
}

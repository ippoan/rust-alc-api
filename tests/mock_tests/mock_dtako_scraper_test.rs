use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use chrono::{NaiveDate, Utc};
use serde_json::Value;

use crate::mock_helpers::app_state::setup_mock_app_state;
use crate::mock_helpers::MockDtakoScraperRepository;
use rust_alc_api::routes::dtako_scraper::ScrapeHistoryItem;

// ============================================================
// GET /api/scraper/history — success (empty)
// ============================================================

#[tokio::test]
async fn test_get_scrape_history_success_empty() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/scraper/history"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Vec<Value> = res.json().await.unwrap();
    assert!(body.is_empty());
}

// ============================================================
// GET /api/scraper/history — success with data
// ============================================================

#[tokio::test]
async fn test_get_scrape_history_with_data() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let item = ScrapeHistoryItem {
        id: Uuid::new_v4(),
        target_date: NaiveDate::from_ymd_opt(2026, 3, 29).unwrap(),
        comp_id: "COMP001".to_string(),
        status: "success".to_string(),
        message: Some("Scraped 10 records".to_string()),
        created_at: Utc::now(),
    };

    let mock = Arc::new(MockDtakoScraperRepository::default());
    mock.history_data.lock().unwrap().push(item);
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/scraper/history"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Vec<Value> = res.json().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["comp_id"], "COMP001");
    assert_eq!(body[0]["status"], "success");
    assert_eq!(body[0]["message"], "Scraped 10 records");
    assert_eq!(body[0]["target_date"], "2026-03-29");
}

// ============================================================
// GET /api/scraper/history — with query params (limit/offset)
// ============================================================

#[tokio::test]
async fn test_get_scrape_history_with_query_params() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/scraper/history?limit=10&offset=5"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

// ============================================================
// GET /api/scraper/history — DB error (500)
// ============================================================

#[tokio::test]
async fn test_get_scrape_history_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockDtakoScraperRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/scraper/history"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// GET /api/scraper/history — unauthorized (no JWT → 401)
// ============================================================

#[tokio::test]
async fn test_get_scrape_history_unauthorized() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/scraper/history"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

// ============================================================
// POST /api/scraper/history — success (front Worker relay が保存)
// ============================================================

#[tokio::test]
async fn test_save_scrape_history_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
    // メタデータサーバーへの接続を即座に失敗させる
    std::env::set_var("GCP_METADATA_URL", "http://127.0.0.1:1");

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mock_ref = mock.clone();
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/scraper/history"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .json(&serde_json::json!({
            "target_date": "2026-03-29",
            "comp_id": "C001",
            "status": "success",
            "message": "Scraped 10 records",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);

    let count = mock_ref.insert_count.load(Ordering::SeqCst);
    assert_eq!(count, 1);
    let comp_ids = mock_ref.inserted_comp_ids.lock().unwrap().clone();
    assert_eq!(comp_ids[0], "C001");
}

// ============================================================
// POST /api/scraper/history — message は optional
// ============================================================

#[tokio::test]
async fn test_save_scrape_history_without_message() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
    std::env::set_var("GCP_METADATA_URL", "http://127.0.0.1:1");

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mock_ref = mock.clone();
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/scraper/history"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .json(&serde_json::json!({
            "target_date": "2026-03-29",
            "comp_id": "C002",
            "status": "error",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);

    let count = mock_ref.insert_count.load(Ordering::SeqCst);
    assert_eq!(count, 1);
}

// ============================================================
// POST /api/scraper/history — DB error (500)
// ============================================================

#[tokio::test]
async fn test_save_scrape_history_db_error() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
    std::env::set_var("GCP_METADATA_URL", "http://127.0.0.1:1");

    let mock = Arc::new(MockDtakoScraperRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/scraper/history"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .json(&serde_json::json!({
            "target_date": "2026-03-29",
            "comp_id": "C003",
            "status": "success",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// POST /api/scraper/history — invalid body (400)
// ============================================================

#[tokio::test]
async fn test_save_scrape_history_invalid_body() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
    std::env::set_var("GCP_METADATA_URL", "http://127.0.0.1:1");

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // target_date が不正な形式 → deserialize 失敗 → 422 (axum Json rejection)
    let res = client
        .post(format!("{base_url}/api/scraper/history"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .json(&serde_json::json!({
            "target_date": "not-a-date",
            "comp_id": "C004",
            "status": "success",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 422);
}

// ============================================================
// POST /api/scraper/history — unauthorized (no JWT → 401)
// ============================================================

#[tokio::test]
async fn test_save_scrape_history_unauthorized() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
    std::env::set_var("GCP_METADATA_URL", "http://127.0.0.1:1");

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/scraper/history"))
        .json(&serde_json::json!({
            "target_date": "2026-03-29",
            "comp_id": "C005",
            "status": "success",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

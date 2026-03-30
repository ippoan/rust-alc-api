#[macro_use]
mod common;
mod mock_helpers;

use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use chrono::{NaiveDate, Utc};
use serde_json::Value;

use mock_helpers::app_state::setup_mock_app_state;
use mock_helpers::MockDtakoScraperRepository;
use rust_alc_api::routes::dtako_scraper::ScrapeHistoryItem;

// ============================================================
// GET /api/scraper/history — success (empty)
// ============================================================

#[tokio::test]
async fn test_get_scrape_history_success_empty() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

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
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDtakoScraperRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = common::create_test_jwt(tenant_id, "admin");
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
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/scraper/history"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

// ============================================================
// POST /api/scraper/trigger — connection refused (502)
// ============================================================

#[tokio::test]
async fn test_trigger_scrape_connection_refused() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);
    // メタデータサーバーへの接続を即座に失敗させる
    std::env::set_var("GCP_METADATA_URL", "http://127.0.0.1:1");

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    // Use port 1 which is guaranteed to refuse connections
    let base_url = common::spawn_test_server_with_scraper(state, "http://127.0.0.1:1").await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/scraper/trigger"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .json(&serde_json::json!({
            "start_date": "2026-03-29",
            "end_date": "2026-03-29",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 502);
    let body = res.text().await.unwrap();
    assert!(body.contains("Scraper connection error"));
}

// ============================================================
// POST /api/scraper/trigger — with optional fields
// ============================================================

#[tokio::test]
async fn test_trigger_scrape_with_all_fields() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);
    std::env::set_var("GCP_METADATA_URL", "http://127.0.0.1:1");

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = common::spawn_test_server_with_scraper(state, "http://127.0.0.1:1").await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/scraper/trigger"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .json(&serde_json::json!({
            "start_date": "2026-03-29",
            "end_date": "2026-03-30",
            "comp_id": "COMP001",
            "skip_upload": true,
        }))
        .send()
        .await
        .unwrap();
    // Still 502 because scraper is not running, but the request body is valid
    assert_eq!(res.status(), 502);
}

// ============================================================
// POST /api/scraper/trigger — empty body (all fields optional)
// ============================================================

#[tokio::test]
async fn test_trigger_scrape_empty_body() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);
    std::env::set_var("GCP_METADATA_URL", "http://127.0.0.1:1");

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = common::spawn_test_server_with_scraper(state, "http://127.0.0.1:1").await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/scraper/trigger"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    // 502 because scraper is not running
    assert_eq!(res.status(), 502);
}

// ============================================================
// POST /api/scraper/trigger — no body (422)
// ============================================================

#[tokio::test]
async fn test_trigger_scrape_no_body() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);
    std::env::set_var("GCP_METADATA_URL", "http://127.0.0.1:1");

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // Send POST with no Content-Type and no body -> should fail deserialization
    let res = client
        .post(format!("{base_url}/api/scraper/trigger"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .header("Content-Type", "application/json")
        .send()
        .await
        .unwrap();
    // Missing body -> 400 Bad Request
    assert_eq!(res.status(), 400);
}

// ============================================================
// POST /api/scraper/trigger — unauthorized (no JWT → 401)
// ============================================================

#[tokio::test]
async fn test_trigger_scrape_unauthorized() {
    let _guard = common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);
    std::env::set_var("GCP_METADATA_URL", "http://127.0.0.1:1");

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = common::spawn_test_server(state).await;

    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/scraper/trigger"))
        .json(&serde_json::json!({
            "start_date": "2026-03-29",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

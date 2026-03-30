use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use chrono::{NaiveDate, Utc};
use serde_json::Value;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::mock_helpers::app_state::setup_mock_app_state;
use crate::mock_helpers::MockDtakoScraperRepository;
use rust_alc_api::routes::dtako_scraper::ScrapeHistoryItem;

// ============================================================
// GET /api/scraper/history — success (empty)
// ============================================================

#[tokio::test]
async fn test_get_scrape_history_success_empty() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
    let base_url = crate::common::spawn_test_server(state).await;

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
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDtakoScraperRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    // メタデータサーバーへの接続を即座に失敗させる
    std::env::set_var("GCP_METADATA_URL", "http://127.0.0.1:1");

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    // Use port 1 which is guaranteed to refuse connections
    let base_url = crate::common::spawn_test_server_with_scraper(state, "http://127.0.0.1:1").await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("GCP_METADATA_URL", "http://127.0.0.1:1");

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = crate::common::spawn_test_server_with_scraper(state, "http://127.0.0.1:1").await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("GCP_METADATA_URL", "http://127.0.0.1:1");

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = crate::common::spawn_test_server_with_scraper(state, "http://127.0.0.1:1").await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("GCP_METADATA_URL", "http://127.0.0.1:1");

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
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
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("GCP_METADATA_URL", "http://127.0.0.1:1");

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url = crate::common::spawn_test_server(state).await;

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

// ============================================================
// POST /api/scraper/trigger — scraper returns 500 (→ 502)
// Covers lines 131-138: non-success status from scraper
// ============================================================

#[tokio::test]
async fn test_trigger_scrape_scraper_returns_500() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    // Mock metadata server (returns 404 so get_id_token fails silently)
    let metadata_server = MockServer::start().await;
    std::env::set_var("GCP_METADATA_URL", metadata_server.uri());

    // Mock scraper server returns 500
    let scraper_server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal scraper error"))
        .mount(&scraper_server)
        .await;

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url =
        crate::common::spawn_test_server_with_scraper(state, &scraper_server.uri()).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/scraper/trigger"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .json(&serde_json::json!({
            "start_date": "2026-03-29",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 502);
    let body = res.text().await.unwrap();
    assert!(body.contains("Scraper returned 500"));
    assert!(body.contains("Internal scraper error"));
}

// ============================================================
// POST /api/scraper/trigger — metadata server returns non-success
// Covers lines 91-93: metadata server returns 403
// ============================================================

#[tokio::test]
async fn test_trigger_scrape_metadata_server_non_success() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    // Mock metadata server returns 403
    let metadata_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(403).set_body_string("Forbidden"))
        .mount(&metadata_server)
        .await;

    // Mock scraper returns 500 (we just need to get past metadata)
    let scraper_server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500).set_body_string("error"))
        .mount(&scraper_server)
        .await;

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url =
        crate::common::spawn_test_server_with_scraper(state, &scraper_server.uri()).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/scraper/trigger"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .json(&serde_json::json!({
            "start_date": "2026-03-29",
        }))
        .send()
        .await
        .unwrap();
    // Metadata error is silent, scraper returns 500 → 502
    assert_eq!(res.status(), 502);
}

// ============================================================
// POST /api/scraper/trigger — metadata server success + bearer auth
// Covers lines 91-97, 121: successful token fetch + bearer_auth set
// ============================================================

#[tokio::test]
async fn test_trigger_scrape_metadata_server_success_bearer_auth() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    // Mock metadata server returns a token
    let metadata_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("fake-id-token-12345"))
        .mount(&metadata_server)
        .await;

    // Mock scraper returns 500 (to simplify; we're testing metadata path)
    let scraper_server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500).set_body_string("server error"))
        .mount(&scraper_server)
        .await;

    std::env::set_var("GCP_METADATA_URL", metadata_server.uri());

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url =
        crate::common::spawn_test_server_with_scraper(state, &scraper_server.uri()).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/scraper/trigger"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .json(&serde_json::json!({
            "start_date": "2026-03-29",
        }))
        .send()
        .await
        .unwrap();
    // Scraper still returns 500 → 502, but metadata token was obtained
    assert_eq!(res.status(), 502);
}

// ============================================================
// POST /api/scraper/trigger — SSE stream with result events
// Covers lines 140-211: full SSE stream processing + DB insert
// ============================================================

#[tokio::test]
async fn test_trigger_scrape_sse_stream_with_results() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("GCP_METADATA_URL", "http://127.0.0.1:1");

    // Mock scraper returns SSE stream with result events
    let scraper_server = MockServer::start().await;
    let sse_body = concat!(
        "data:{\"event\":\"progress\",\"message\":\"Starting...\"}\n\n",
        "data:{\"event\":\"result\",\"comp_id\":\"C001\",\"status\":\"success\",\"message\":\"Done\"}\n\n",
        "data:{\"event\":\"result\",\"comp_id\":\"C002\",\"status\":\"error\",\"message\":\"Failed\"}\n\n",
    );
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_body)
                .append_header("content-type", "text/event-stream"),
        )
        .mount(&scraper_server)
        .await;

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mock_ref = mock.clone();
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url =
        crate::common::spawn_test_server_with_scraper(state, &scraper_server.uri()).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/scraper/trigger"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .json(&serde_json::json!({
            "start_date": "2026-03-29",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // Read the SSE stream body
    let body = res.text().await.unwrap();
    // Should contain the relayed events
    assert!(body.contains("Starting..."));
    assert!(body.contains("C001"));
    assert!(body.contains("C002"));

    // Wait briefly for the spawned task to complete DB inserts
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Verify DB inserts: 2 result events → 2 insert calls
    let count = mock_ref
        .insert_count
        .load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(count, 2, "Expected 2 insert_scrape_history calls");

    let comp_ids = mock_ref.inserted_comp_ids.lock().unwrap().clone();
    assert!(comp_ids.contains(&"C001".to_string()));
    assert!(comp_ids.contains(&"C002".to_string()));
}

// ============================================================
// POST /api/scraper/trigger — SSE stream without start_date
// Covers lines 140-146: default target_date (yesterday)
// ============================================================

#[tokio::test]
async fn test_trigger_scrape_sse_stream_no_start_date() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("GCP_METADATA_URL", "http://127.0.0.1:1");

    let scraper_server = MockServer::start().await;
    let sse_body = "data:{\"event\":\"result\",\"comp_id\":\"C099\",\"status\":\"success\",\"message\":\"ok\"}\n\n";
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_body)
                .append_header("content-type", "text/event-stream"),
        )
        .mount(&scraper_server)
        .await;

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mock_ref = mock.clone();
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url =
        crate::common::spawn_test_server_with_scraper(state, &scraper_server.uri()).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // No start_date → uses yesterday as default
    let res = client
        .post(format!("{base_url}/api/scraper/trigger"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body = res.text().await.unwrap();
    assert!(body.contains("C099"));

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let count = mock_ref
        .insert_count
        .load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(count, 1);
}

// ============================================================
// POST /api/scraper/trigger — SSE stream with invalid date
// Covers line 146: invalid date falls back to today
// ============================================================

#[tokio::test]
async fn test_trigger_scrape_sse_stream_invalid_date() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("GCP_METADATA_URL", "http://127.0.0.1:1");

    let scraper_server = MockServer::start().await;
    let sse_body = "data:{\"event\":\"result\",\"comp_id\":\"C100\",\"status\":\"success\",\"message\":\"ok\"}\n\n";
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_body)
                .append_header("content-type", "text/event-stream"),
        )
        .mount(&scraper_server)
        .await;

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mock_ref = mock.clone();
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url =
        crate::common::spawn_test_server_with_scraper(state, &scraper_server.uri()).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // Invalid date → fallback to today
    let res = client
        .post(format!("{base_url}/api/scraper/trigger"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .json(&serde_json::json!({
            "start_date": "not-a-date",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body = res.text().await.unwrap();
    assert!(body.contains("C100"));

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let count = mock_ref
        .insert_count
        .load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(count, 1);
}

// ============================================================
// POST /api/scraper/trigger — SSE with non-JSON data
// Covers line 179: serde_json::from_str fails, event still relayed
// ============================================================

#[tokio::test]
async fn test_trigger_scrape_sse_stream_non_json_data() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("GCP_METADATA_URL", "http://127.0.0.1:1");

    let scraper_server = MockServer::start().await;
    // Mix of valid JSON and non-JSON data
    let sse_body = concat!(
        "data:this is not json\n\n",
        "data:{\"event\":\"result\",\"comp_id\":\"C003\",\"status\":\"success\",\"message\":\"ok\"}\n\n",
    );
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_body)
                .append_header("content-type", "text/event-stream"),
        )
        .mount(&scraper_server)
        .await;

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mock_ref = mock.clone();
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url =
        crate::common::spawn_test_server_with_scraper(state, &scraper_server.uri()).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/scraper/trigger"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .json(&serde_json::json!({
            "start_date": "2026-03-29",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body = res.text().await.unwrap();
    // Both events should be relayed (non-JSON is still sent as data)
    assert!(body.contains("this is not json"));
    assert!(body.contains("C003"));

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    // Only 1 DB insert (the valid result event)
    let count = mock_ref
        .insert_count
        .load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(count, 1);
}

// ============================================================
// POST /api/scraper/trigger — SSE with progress event (no DB insert)
// Covers lines 180: event != "result" → no DB insert
// ============================================================

#[tokio::test]
async fn test_trigger_scrape_sse_stream_progress_only() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("GCP_METADATA_URL", "http://127.0.0.1:1");

    let scraper_server = MockServer::start().await;
    let sse_body = concat!(
        "data:{\"event\":\"progress\",\"message\":\"Step 1\"}\n\n",
        "data:{\"event\":\"progress\",\"message\":\"Step 2\"}\n\n",
    );
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_body)
                .append_header("content-type", "text/event-stream"),
        )
        .mount(&scraper_server)
        .await;

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mock_ref = mock.clone();
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url =
        crate::common::spawn_test_server_with_scraper(state, &scraper_server.uri()).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/scraper/trigger"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .json(&serde_json::json!({
            "start_date": "2026-03-29",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body = res.text().await.unwrap();
    assert!(body.contains("Step 1"));
    assert!(body.contains("Step 2"));

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    // No result events → 0 DB inserts
    let count = mock_ref
        .insert_count
        .load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(count, 0);
}

// ============================================================
// POST /api/scraper/trigger — SSE result without comp_id
// Covers line 181: result event but comp_id is None → no DB insert
// ============================================================

#[tokio::test]
async fn test_trigger_scrape_sse_result_no_comp_id() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("GCP_METADATA_URL", "http://127.0.0.1:1");

    let scraper_server = MockServer::start().await;
    // result event without comp_id
    let sse_body =
        "data:{\"event\":\"result\",\"status\":\"success\",\"message\":\"no comp_id\"}\n\n";
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_body)
                .append_header("content-type", "text/event-stream"),
        )
        .mount(&scraper_server)
        .await;

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mock_ref = mock.clone();
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url =
        crate::common::spawn_test_server_with_scraper(state, &scraper_server.uri()).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/scraper/trigger"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .json(&serde_json::json!({
            "start_date": "2026-03-29",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body = res.text().await.unwrap();
    assert!(body.contains("no comp_id"));

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    // No comp_id → 0 DB inserts
    let count = mock_ref
        .insert_count
        .load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(count, 0);
}

// ============================================================
// POST /api/scraper/trigger — SSE result with missing status field
// Covers line 182: status is None → defaults to "error"
// ============================================================

#[tokio::test]
async fn test_trigger_scrape_sse_result_missing_status() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("GCP_METADATA_URL", "http://127.0.0.1:1");

    let scraper_server = MockServer::start().await;
    // result with comp_id but no status/message
    let sse_body = "data:{\"event\":\"result\",\"comp_id\":\"C004\"}\n\n";
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_body)
                .append_header("content-type", "text/event-stream"),
        )
        .mount(&scraper_server)
        .await;

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mock_ref = mock.clone();
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url =
        crate::common::spawn_test_server_with_scraper(state, &scraper_server.uri()).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/scraper/trigger"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .json(&serde_json::json!({
            "start_date": "2026-03-29",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body = res.text().await.unwrap();
    assert!(body.contains("C004"));

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let count = mock_ref
        .insert_count
        .load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(count, 1);
    let comp_ids = mock_ref.inserted_comp_ids.lock().unwrap().clone();
    assert_eq!(comp_ids[0], "C004");
}

// ============================================================
// POST /api/scraper/trigger — SSE with empty data lines
// Covers line 176: empty data after strip_prefix is skipped
// ============================================================

#[tokio::test]
async fn test_trigger_scrape_sse_stream_empty_data() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("GCP_METADATA_URL", "http://127.0.0.1:1");

    let scraper_server = MockServer::start().await;
    // Empty data line followed by a real event
    let sse_body = concat!(
        "data:\n\n",
        "data:{\"event\":\"result\",\"comp_id\":\"C005\",\"status\":\"success\",\"message\":\"ok\"}\n\n",
    );
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_body)
                .append_header("content-type", "text/event-stream"),
        )
        .mount(&scraper_server)
        .await;

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mock_ref = mock.clone();
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url =
        crate::common::spawn_test_server_with_scraper(state, &scraper_server.uri()).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/scraper/trigger"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .json(&serde_json::json!({
            "start_date": "2026-03-29",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body = res.text().await.unwrap();
    assert!(body.contains("C005"));

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let count = mock_ref
        .insert_count
        .load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(count, 1);
}

// ============================================================
// POST /api/scraper/trigger — metadata success + SSE stream
// Full end-to-end: metadata OK → bearer auth → scraper SSE → DB save
// Covers lines 91-97, 121, 140-211 together
// ============================================================

#[tokio::test]
async fn test_trigger_scrape_full_flow_with_metadata_and_sse() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    // Mock metadata server returns a valid token
    let metadata_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("my-id-token"))
        .mount(&metadata_server)
        .await;
    std::env::set_var("GCP_METADATA_URL", metadata_server.uri());

    // Mock scraper returns SSE
    let scraper_server = MockServer::start().await;
    let sse_body = "data:{\"event\":\"result\",\"comp_id\":\"C010\",\"status\":\"success\",\"message\":\"Full flow\"}\n\n";
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_body)
                .append_header("content-type", "text/event-stream"),
        )
        .mount(&scraper_server)
        .await;

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mock_ref = mock.clone();
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url =
        crate::common::spawn_test_server_with_scraper(state, &scraper_server.uri()).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/scraper/trigger"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .json(&serde_json::json!({
            "start_date": "2026-03-29",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body = res.text().await.unwrap();
    assert!(body.contains("Full flow"));

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let count = mock_ref
        .insert_count
        .load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(count, 1);
    let comp_ids = mock_ref.inserted_comp_ids.lock().unwrap().clone();
    assert_eq!(comp_ids[0], "C010");
}

// ============================================================
// POST /api/scraper/trigger — SSE with non-data lines (e.g., comments)
// Covers line 174: lines without "data:" prefix are ignored
// ============================================================

#[tokio::test]
async fn test_trigger_scrape_sse_stream_with_comments() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::set_var("GCP_METADATA_URL", "http://127.0.0.1:1");

    let scraper_server = MockServer::start().await;
    // SSE with comment lines (: prefix) and event/id fields
    let sse_body = concat!(
        ": this is a comment\n",
        "id: 1\n",
        "event: message\n",
        "data:{\"event\":\"result\",\"comp_id\":\"C006\",\"status\":\"success\",\"message\":\"ok\"}\n\n",
    );
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_body)
                .append_header("content-type", "text/event-stream"),
        )
        .mount(&scraper_server)
        .await;

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mock_ref = mock.clone();
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url =
        crate::common::spawn_test_server_with_scraper(state, &scraper_server.uri()).await;

    let tenant_id = Uuid::new_v4();
    let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/scraper/trigger"))
        .header("Authorization", format!("Bearer {admin_jwt}"))
        .json(&serde_json::json!({
            "start_date": "2026-03-29",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body = res.text().await.unwrap();
    assert!(body.contains("C006"));

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let count = mock_ref
        .insert_count
        .load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(count, 1);
}

/// SSE client disconnect — レスポンスを読まずに drop → tx.send() 失敗 (line 198-199)
#[tokio::test]
async fn test_trigger_scrape_client_disconnect() {
    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

    let scraper_server = MockServer::start().await;
    std::env::set_var("SCRAPER_BASE_URL", scraper_server.uri());
    std::env::remove_var("METADATA_URL");

    // 大量のイベントを返す
    let mut sse_body = String::new();
    for i in 0..100 {
        sse_body.push_str(&format!(
            "data:{{\"event\":\"result\",\"comp_id\":\"DISC{i:03}\",\"status\":\"success\"}}\n\n"
        ));
    }

    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse_body),
        )
        .mount(&scraper_server)
        .await;

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url =
        crate::common::spawn_test_server_with_scraper(state, &scraper_server.uri()).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/scraper/trigger"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({"start_date": "2026-03-20"}))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    // レスポンスを読まずに即 drop → tx.send() が Err → "client disconnected"
    drop(res);

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
}

/// SSE stream error — 不正な chunked encoding (line 161-163)
#[tokio::test]
async fn test_trigger_scrape_stream_error() {
    use tokio::io::AsyncWriteExt;

    let _guard = crate::common::ENV_LOCK.lock().unwrap();
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    std::env::remove_var("METADATA_URL");

    // 生 TCP サーバーで不正な chunked レスポンスを返す
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    std::env::set_var("SCRAPER_BASE_URL", format!("http://{addr}"));

    tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = [0u8; 4096];
            let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await;

            let header = "HTTP/1.1 200 OK\r\n\
                          Content-Type: text/event-stream\r\n\
                          Transfer-Encoding: chunked\r\n\r\n";
            let _ = stream.write_all(header.as_bytes()).await;

            // 正常なチャンク1つ
            let chunk_data = "data:{\"event\":\"progress\",\"message\":\"ok\"}\n\n";
            let chunk = format!("{:x}\r\n{}\r\n", chunk_data.len(), chunk_data);
            let _ = stream.write_all(chunk.as_bytes()).await;
            let _ = stream.flush().await;

            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            // 不正なチャンクサイズ → bytes_stream Err
            let _ = stream.write_all(b"FFFFFF\r\n").await;
            let _ = stream.flush().await;
            drop(stream);
        }
    });

    let mock = Arc::new(MockDtakoScraperRepository::default());
    let mut state = setup_mock_app_state();
    state.dtako_scraper = mock;
    let base_url =
        crate::common::spawn_test_server_with_scraper(state, &format!("http://{addr}")).await;

    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");

    let client = reqwest::Client::new();
    let res = client
        .post(format!("{base_url}/api/scraper/trigger"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({"start_date": "2026-03-20"}))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let _body = res.text().await.unwrap_or_default();

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
}

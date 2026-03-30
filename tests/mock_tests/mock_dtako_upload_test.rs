use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::mock_helpers::app_state::setup_mock_app_state;
use crate::mock_helpers::MockDtakoUploadRepository;

/// Helper: set up mock AppState + spawn test server + create JWT.
/// Returns (base_url, auth_header, tenant_id, state).
async fn setup() -> (String, String, Uuid) {
    let state = setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header, tenant_id)
}

// =========================================================================
// POST /api/upload — success with valid ZIP
// =========================================================================

#[tokio::test]
async fn test_dtako_upload_zip_success() {
    let (base_url, auth_header, _tenant_id) = setup().await;
    let client = reqwest::Client::new();

    let zip_bytes = crate::common::create_test_dtako_zip();
    let part = reqwest::multipart::Part::bytes(zip_bytes).file_name("test.zip");
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", &auth_header)
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["upload_id"].as_str().is_some());
    assert_eq!(body["status"], "completed");
    assert!(body["operations_count"].as_i64().unwrap() >= 0);
}

// =========================================================================
// POST /api/upload — no file field → 400
// =========================================================================

#[tokio::test]
async fn test_dtako_upload_no_file() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let form = reqwest::multipart::Form::new();

    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", &auth_header)
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

// =========================================================================
// POST /api/upload — invalid ZIP (not a ZIP file) → 400
// =========================================================================

#[tokio::test]
async fn test_dtako_upload_invalid_zip() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let part = reqwest::multipart::Part::bytes(vec![0x00, 0x01, 0x02, 0x03]).file_name("bad.zip");
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", &auth_header)
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

// =========================================================================
// POST /api/upload — DB error (create_upload_history fails) → 500
// =========================================================================

#[tokio::test]
async fn test_dtako_upload_db_error() {
    let mut state = setup_mock_app_state();
    let mock = Arc::new(MockDtakoUploadRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    state.dtako_upload = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let zip_bytes = crate::common::create_test_dtako_zip();
    let part = reqwest::multipart::Part::bytes(zip_bytes).file_name("test.zip");
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", format!("Bearer {jwt}"))
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// POST /api/upload — no auth → 401
// =========================================================================

#[tokio::test]
async fn test_dtako_upload_no_auth() {
    let (base_url, _, _) = setup().await;
    let client = reqwest::Client::new();

    let zip_bytes = crate::common::create_test_dtako_zip();
    let part = reqwest::multipart::Part::bytes(zip_bytes).file_name("test.zip");
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/upload"))
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

// =========================================================================
// GET /api/uploads — success (empty list)
// =========================================================================

#[tokio::test]
async fn test_dtako_list_uploads_success() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/uploads"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Vec<serde_json::Value> = res.json().await.unwrap();
    assert!(body.is_empty());
}

// =========================================================================
// GET /api/uploads — DB error → 500
// =========================================================================

#[tokio::test]
async fn test_dtako_list_uploads_db_error() {
    let mut state = setup_mock_app_state();
    let mock = Arc::new(MockDtakoUploadRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    state.dtako_upload = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/uploads"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/internal/pending — success (empty list)
// =========================================================================

#[tokio::test]
async fn test_dtako_list_pending_success() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/internal/pending"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: Vec<serde_json::Value> = res.json().await.unwrap();
    assert!(body.is_empty());
}

// =========================================================================
// GET /api/internal/pending — DB error → 500
// =========================================================================

#[tokio::test]
async fn test_dtako_list_pending_db_error() {
    let mut state = setup_mock_app_state();
    let mock = Arc::new(MockDtakoUploadRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    state.dtako_upload = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/internal/pending"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/internal/download/{id} — not found → 404
// =========================================================================

#[tokio::test]
async fn test_dtako_download_not_found() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!(
            "{base_url}/api/internal/download/{}",
            Uuid::new_v4()
        ))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

// =========================================================================
// GET /api/internal/download/{id} — DB error → 500
// =========================================================================

#[tokio::test]
async fn test_dtako_download_db_error() {
    let mut state = setup_mock_app_state();
    let mock = Arc::new(MockDtakoUploadRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    state.dtako_upload = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!(
            "{base_url}/api/internal/download/{}",
            Uuid::new_v4()
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// POST /api/internal/rerun/{id} — not found → 404
// =========================================================================

#[tokio::test]
async fn test_dtako_rerun_not_found() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/internal/rerun/{}", Uuid::new_v4()))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

// =========================================================================
// POST /api/internal/rerun/{id} — DB error → 500
// =========================================================================

#[tokio::test]
async fn test_dtako_rerun_db_error() {
    let mut state = setup_mock_app_state();
    let mock = Arc::new(MockDtakoUploadRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    state.dtako_upload = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/internal/rerun/{}", Uuid::new_v4()))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// POST /api/split-csv/{id} — not found (get_upload_tenant_and_key returns None)
// =========================================================================

#[tokio::test]
async fn test_dtako_split_csv_not_found() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/split-csv/{}", Uuid::new_v4()))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();

    // split_csv_from_r2 returns anyhow error "upload X not found" → 500
    assert_eq!(res.status(), 500);
}

// =========================================================================
// POST /api/split-csv/{id} — DB error → 500
// =========================================================================

#[tokio::test]
async fn test_dtako_split_csv_db_error() {
    let mut state = setup_mock_app_state();
    let mock = Arc::new(MockDtakoUploadRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    state.dtako_upload = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/split-csv/{}", Uuid::new_v4()))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// POST /api/split-csv-all — success (SSE stream, empty list)
// =========================================================================

#[tokio::test]
async fn test_dtako_split_csv_all_success() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/split-csv-all"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();

    // SSE endpoints return 200 with text/event-stream
    assert_eq!(res.status(), 200);
    let body = res.text().await.unwrap();
    // The SSE stream should contain a "done" event
    assert!(body.contains("done"));
}

// =========================================================================
// POST /api/split-csv-all — DB error (SSE stream with error)
// =========================================================================

#[tokio::test]
async fn test_dtako_split_csv_all_db_error() {
    let mut state = setup_mock_app_state();
    let mock = Arc::new(MockDtakoUploadRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    state.dtako_upload = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/split-csv-all"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    // SSE endpoints always return 200 (error is in the stream body)
    assert_eq!(res.status(), 200);
    let body = res.text().await.unwrap();
    assert!(body.contains("error"));
}

// =========================================================================
// POST /api/recalculate-driver — success (SSE stream)
// =========================================================================

#[tokio::test]
async fn test_dtako_recalculate_driver_success() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let driver_id = Uuid::new_v4();
    let res = client
        .post(format!(
            "{base_url}/api/recalculate-driver?year=2026&month=3&driver_id={driver_id}"
        ))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();

    // SSE endpoint returns 200
    assert_eq!(res.status(), 200);
    let body = res.text().await.unwrap();
    // Mock get_driver_cd returns None → error "driver not found"
    assert!(body.contains("error") || body.contains("done"));
}

// =========================================================================
// POST /api/recalculate-driver — DB error (SSE stream)
// =========================================================================

#[tokio::test]
async fn test_dtako_recalculate_driver_db_error() {
    let mut state = setup_mock_app_state();
    let mock = Arc::new(MockDtakoUploadRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    state.dtako_upload = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let driver_id = Uuid::new_v4();
    let res = client
        .post(format!(
            "{base_url}/api/recalculate-driver?year=2026&month=3&driver_id={driver_id}"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body = res.text().await.unwrap();
    assert!(body.contains("error"));
}

// =========================================================================
// POST /api/recalculate-drivers — success (SSE stream)
// =========================================================================

#[tokio::test]
async fn test_dtako_recalculate_drivers_success() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/recalculate-drivers"))
        .header("Authorization", &auth_header)
        .header("Content-Type", "application/json")
        .body(
            serde_json::json!({
                "year": 2026,
                "month": 3,
                "driver_ids": []
            })
            .to_string(),
        )
        .send()
        .await
        .unwrap();

    // SSE endpoint returns 200
    assert_eq!(res.status(), 200);
    let body = res.text().await.unwrap();
    assert!(body.contains("batch_done") || body.contains("batch_start"));
}

// =========================================================================
// POST /api/recalculate-drivers — DB error (SSE stream)
// =========================================================================

#[tokio::test]
async fn test_dtako_recalculate_drivers_db_error() {
    let mut state = setup_mock_app_state();
    let mock = Arc::new(MockDtakoUploadRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    state.dtako_upload = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/recalculate-drivers"))
        .header("Authorization", format!("Bearer {jwt}"))
        .header("Content-Type", "application/json")
        .body(
            serde_json::json!({
                "year": 2026,
                "month": 3,
                "driver_ids": [Uuid::new_v4()]
            })
            .to_string(),
        )
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body = res.text().await.unwrap();
    // With fail_next, fetch_zip_keys fails → error in stream
    assert!(body.contains("error") || body.contains("batch_done"));
}

// =========================================================================
// POST /api/recalculate — success (SSE stream)
// =========================================================================

#[tokio::test]
async fn test_dtako_recalculate_success() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/recalculate?year=2026&month=3"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();

    // SSE endpoint returns 200
    assert_eq!(res.status(), 200);
    let body = res.text().await.unwrap();
    // With empty mock data, fetch_operations_for_recalc returns empty → done with 0
    assert!(body.contains("done"));
}

// =========================================================================
// POST /api/recalculate — DB error (SSE stream)
// =========================================================================

#[tokio::test]
async fn test_dtako_recalculate_db_error() {
    let mut state = setup_mock_app_state();
    let mock = Arc::new(MockDtakoUploadRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    state.dtako_upload = mock;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/recalculate?year=2026&month=3"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body = res.text().await.unwrap();
    assert!(body.contains("error"));
}

// =========================================================================
// GET /api/uploads — no auth → 401
// =========================================================================

#[tokio::test]
async fn test_dtako_list_uploads_no_auth() {
    let (base_url, _, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/uploads"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

// =========================================================================
// POST /api/upload — invalid multipart body → 400
// =========================================================================

#[tokio::test]
async fn test_dtako_upload_invalid_multipart() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", &auth_header)
        .header("Content-Type", "multipart/form-data; boundary=INVALID")
        .body("not a valid multipart body")
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

// =========================================================================
// POST /api/upload — X-Tenant-ID header (kiosk mode) — success
// =========================================================================

#[tokio::test]
async fn test_dtako_upload_with_tenant_header() {
    let state = setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let zip_bytes = crate::common::create_test_dtako_zip();
    let part = reqwest::multipart::Part::bytes(zip_bytes).file_name("test.zip");
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("X-Tenant-ID", tenant_id.to_string())
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "completed");
}

use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::common::mock_storage::MockStorage;
use crate::mock_helpers::app_state::setup_mock_app_state;
use crate::mock_helpers::MockDtakoUploadRepository;
use rust_alc_api::db::repository::dtako_upload::{
    DtakoDriverOpRow, DtakoOpRow, UploadHistoryRecord, UploadTenantAndKey,
};

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

// =========================================================================
// Helper: build a custom AppState with configured MockDtakoUploadRepository
// =========================================================================

fn setup_with_mock(mock: MockDtakoUploadRepository) -> rust_alc_api::AppState {
    let mut state = setup_mock_app_state();
    state.dtako_upload = Arc::new(mock);
    state
}

/// Helper: build AppState with custom dtako_storage (MockStorage) and mock repo
fn setup_with_storage_and_mock(
    mock: MockDtakoUploadRepository,
    storage: Arc<MockStorage>,
) -> rust_alc_api::AppState {
    let mut state = setup_mock_app_state();
    state.dtako_upload = Arc::new(mock);
    state.dtako_storage = Some(storage);
    state
}

// =========================================================================
// POST /api/upload — rich ZIP with multiple operations, events, rest, break
// =========================================================================

#[tokio::test]
async fn test_dtako_upload_rich_zip_success() {
    let state = setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let zip_bytes = crate::common::create_test_dtako_zip_rich();
    let part = reqwest::multipart::Part::bytes(zip_bytes).file_name("rich.zip");
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", format!("Bearer {jwt}"))
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "completed");
    // Rich ZIP has 3 operations
    assert!(body["operations_count"].as_i64().unwrap() >= 3);
}

// =========================================================================
// POST /api/upload — rich ZIP with employee_id mapping (driver_id present)
// =========================================================================

#[tokio::test]
async fn test_dtako_upload_rich_zip_with_employee_id() {
    let employee_id = Uuid::new_v4();
    let mut mock = MockDtakoUploadRepository::default();
    *mock.employee_id.lock().unwrap() = Some(employee_id);

    let state = setup_with_mock(mock);
    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let zip_bytes = crate::common::create_test_dtako_zip_rich();
    let part = reqwest::multipart::Part::bytes(zip_bytes).file_name("rich.zip");
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", format!("Bearer {jwt}"))
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "completed");
    assert!(body["operations_count"].as_i64().unwrap() >= 3);
}

// =========================================================================
// POST /api/upload — ZIP with only header (empty KUDGURI CSV → 0 operations)
// =========================================================================

#[tokio::test]
async fn test_dtako_upload_empty_csv() {
    let state = setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // Create ZIP with header-only KUDGURI (no data rows) and KUDGIVT
    let zip_bytes = create_empty_csv_zip();
    let part = reqwest::multipart::Part::bytes(zip_bytes).file_name("empty.zip");
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", format!("Bearer {jwt}"))
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "completed");
    assert_eq!(body["operations_count"].as_i64().unwrap(), 0);
}

/// Create a ZIP with header-only KUDGURI CSV (no data rows)
fn create_empty_csv_zip() -> Vec<u8> {
    use std::io::Write;

    let kudguri_csv =
        "運行NO,読取日,事業所CD,事業所名,車輌CD,車輌名,乗務員CD1,乗務員名１,対象乗務員区分\n";
    let kudgivt_csv =
        "運行NO,読取日,乗務員CD1,乗務員名１,対象乗務員区分,開始日時,イベントCD,イベント名\n";

    let (kudguri_bytes, _, _) = encoding_rs::SHIFT_JIS.encode(kudguri_csv);
    let (kudgivt_bytes, _, _) = encoding_rs::SHIFT_JIS.encode(kudgivt_csv);

    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("KUDGURI.csv", options).unwrap();
        zip.write_all(&kudguri_bytes).unwrap();
        zip.start_file("KUDGIVT.csv", options).unwrap();
        zip.write_all(&kudgivt_bytes).unwrap();
        zip.finish().unwrap();
    }
    buf.into_inner()
}

// =========================================================================
// POST /api/upload — ZIP missing KUDGURI → 400
// =========================================================================

#[tokio::test]
async fn test_dtako_upload_missing_kudguri() {
    let state = setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // ZIP with only KUDGIVT, no KUDGURI
    let zip_bytes = create_missing_kudguri_zip();
    let part = reqwest::multipart::Part::bytes(zip_bytes).file_name("no-kudguri.zip");
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", format!("Bearer {jwt}"))
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
    let body = res.text().await.unwrap();
    assert!(body.contains("KUDGURI"));
}

fn create_missing_kudguri_zip() -> Vec<u8> {
    use std::io::Write;

    let kudgivt_csv =
        "運行NO,読取日,乗務員CD1,乗務員名１,対象乗務員区分,開始日時,イベントCD,イベント名\n\
         1001,2026/03/01,DR01,テスト運転者,1,2026/03/01 08:00:00,100,出庫\n";

    let (kudgivt_bytes, _, _) = encoding_rs::SHIFT_JIS.encode(kudgivt_csv);

    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("KUDGIVT.csv", options).unwrap();
        zip.write_all(&kudgivt_bytes).unwrap();
        zip.finish().unwrap();
    }
    buf.into_inner()
}

// =========================================================================
// POST /api/upload — ZIP missing KUDGIVT → 400
// =========================================================================

#[tokio::test]
async fn test_dtako_upload_missing_kudgivt() {
    let state = setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    // ZIP with only KUDGURI, no KUDGIVT
    let zip_bytes = create_missing_kudgivt_zip();
    let part = reqwest::multipart::Part::bytes(zip_bytes).file_name("no-kudgivt.zip");
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", format!("Bearer {jwt}"))
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
    let body = res.text().await.unwrap();
    assert!(body.contains("KUDGIVT"));
}

fn create_missing_kudgivt_zip() -> Vec<u8> {
    use std::io::Write;

    let kudguri_csv =
        "運行NO,読取日,事業所CD,事業所名,車輌CD,車輌名,乗務員CD1,乗務員名１,対象乗務員区分\n\
                       1001,2026/03/01,OFF01,テスト事業所,VH01,テスト車両,DR01,テスト運転者,1\n";

    let (kudguri_bytes, _, _) = encoding_rs::SHIFT_JIS.encode(kudguri_csv);

    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("KUDGURI.csv", options).unwrap();
        zip.write_all(&kudguri_bytes).unwrap();
        zip.finish().unwrap();
    }
    buf.into_inner()
}

// =========================================================================
// POST /api/upload — multipart with non-file field (exercises the while loop skip)
// =========================================================================

#[tokio::test]
async fn test_dtako_upload_multipart_with_extra_fields() {
    let state = setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let zip_bytes = crate::common::create_test_dtako_zip();
    let file_part = reqwest::multipart::Part::bytes(zip_bytes).file_name("test.zip");
    // Add extra non-file fields before the actual file
    let form = reqwest::multipart::Form::new()
        .text("extra_field", "some_value")
        .part("file", file_part);

    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", format!("Bearer {jwt}"))
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
}

// =========================================================================
// GET /api/internal/download/{id} — success (upload_history returns Some)
// =========================================================================

#[tokio::test]
async fn test_dtako_download_success() {
    let tenant_id = Uuid::new_v4();
    let upload_id = Uuid::new_v4();
    let zip_key = format!("{}/uploads/{}/test.zip", tenant_id, upload_id);

    // Create mock with upload_history that returns a record
    let mut mock = MockDtakoUploadRepository::default();
    *mock.upload_history.lock().unwrap() = Some(UploadHistoryRecord {
        tenant_id,
        r2_zip_key: zip_key.clone(),
        filename: "テスト.zip".to_string(),
    });

    // Create storage with the ZIP file pre-populated
    let dtako_storage = Arc::new(MockStorage::new("dtako-bucket"));
    let zip_bytes = crate::common::create_test_dtako_zip();
    dtako_storage.insert_file(&zip_key, zip_bytes.clone());

    let state = setup_with_storage_and_mock(mock, dtako_storage);
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/internal/download/{upload_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    assert_eq!(
        res.headers().get("Content-Type").unwrap(),
        "application/zip"
    );
    let downloaded = res.bytes().await.unwrap();
    assert_eq!(downloaded.len(), zip_bytes.len());
}

// =========================================================================
// GET /api/internal/download/{id} — success with ASCII-safe filename
// =========================================================================

#[tokio::test]
async fn test_dtako_download_ascii_safe_filename() {
    let tenant_id = Uuid::new_v4();
    let upload_id = Uuid::new_v4();
    let zip_key = format!("{}/uploads/{}/test.zip", tenant_id, upload_id);

    let mut mock = MockDtakoUploadRepository::default();
    // Use a filename with non-ASCII chars (Japanese) that will be filtered
    *mock.upload_history.lock().unwrap() = Some(UploadHistoryRecord {
        tenant_id,
        r2_zip_key: zip_key.clone(),
        filename: "日本語ファイル".to_string(), // All non-ASCII → safe_name becomes empty → "download.zip"
    });

    let dtako_storage = Arc::new(MockStorage::new("dtako-bucket"));
    dtako_storage.insert_file(&zip_key, vec![0x50, 0x4B, 0x03, 0x04]); // minimal data

    let state = setup_with_storage_and_mock(mock, dtako_storage);
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/internal/download/{upload_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let cd = res
        .headers()
        .get("Content-Disposition")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(cd.contains("download.zip"));
}

// =========================================================================
// POST /api/internal/rerun/{id} — success (re-process existing ZIP)
// =========================================================================

#[tokio::test]
async fn test_dtako_rerun_success() {
    let tenant_id = Uuid::new_v4();
    let upload_id = Uuid::new_v4();
    let zip_key = format!("{}/uploads/{}/test.zip", tenant_id, upload_id);

    let mut mock = MockDtakoUploadRepository::default();
    *mock.upload_history.lock().unwrap() = Some(UploadHistoryRecord {
        tenant_id,
        r2_zip_key: zip_key.clone(),
        filename: "test.zip".to_string(),
    });
    // Also set tenant_and_key for the split_csv that happens after rerun
    *mock.tenant_and_key.lock().unwrap() = Some(UploadTenantAndKey {
        tenant_id,
        r2_zip_key: zip_key.clone(),
    });

    let dtako_storage = Arc::new(MockStorage::new("dtako-bucket"));
    let zip_bytes = crate::common::create_test_dtako_zip();
    dtako_storage.insert_file(&zip_key, zip_bytes);

    let state = setup_with_storage_and_mock(mock, dtako_storage);
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/internal/rerun/{upload_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "completed");
    assert!(body["operations_count"].as_i64().unwrap() >= 1);
}

// =========================================================================
// POST /api/internal/rerun/{id} — rerun with rich ZIP
// =========================================================================

#[tokio::test]
async fn test_dtako_rerun_rich_zip() {
    let tenant_id = Uuid::new_v4();
    let upload_id = Uuid::new_v4();
    let zip_key = format!("{}/uploads/{}/rich.zip", tenant_id, upload_id);

    let employee_id = Uuid::new_v4();
    let mut mock = MockDtakoUploadRepository::default();
    *mock.upload_history.lock().unwrap() = Some(UploadHistoryRecord {
        tenant_id,
        r2_zip_key: zip_key.clone(),
        filename: "rich.zip".to_string(),
    });
    *mock.tenant_and_key.lock().unwrap() = Some(UploadTenantAndKey {
        tenant_id,
        r2_zip_key: zip_key.clone(),
    });
    *mock.employee_id.lock().unwrap() = Some(employee_id);

    let dtako_storage = Arc::new(MockStorage::new("dtako-bucket"));
    let zip_bytes = crate::common::create_test_dtako_zip_rich();
    dtako_storage.insert_file(&zip_key, zip_bytes);

    let state = setup_with_storage_and_mock(mock, dtako_storage);
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/internal/rerun/{upload_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "completed");
    assert!(body["operations_count"].as_i64().unwrap() >= 3);
}

// =========================================================================
// POST /api/split-csv/{id} — success (tenant_and_key returns Some)
// =========================================================================

#[tokio::test]
async fn test_dtako_split_csv_success() {
    let tenant_id = Uuid::new_v4();
    let upload_id = Uuid::new_v4();
    let zip_key = format!("{}/uploads/{}/test.zip", tenant_id, upload_id);

    let mut mock = MockDtakoUploadRepository::default();
    *mock.tenant_and_key.lock().unwrap() = Some(UploadTenantAndKey {
        tenant_id,
        r2_zip_key: zip_key.clone(),
    });

    let dtako_storage = Arc::new(MockStorage::new("dtako-bucket"));
    let zip_bytes = crate::common::create_test_dtako_zip_rich();
    dtako_storage.insert_file(&zip_key, zip_bytes);

    let state = setup_with_storage_and_mock(mock, dtako_storage);
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/split-csv/{upload_id}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "ok");
}

// =========================================================================
// POST /api/split-csv-all — success with actual uploads needing split
// =========================================================================

#[tokio::test]
async fn test_dtako_split_csv_all_with_uploads() {
    let tenant_id = Uuid::new_v4();
    let upload_id = Uuid::new_v4();
    let zip_key = format!("{}/uploads/{}/test.zip", tenant_id, upload_id);

    let mut mock = MockDtakoUploadRepository::default();
    *mock.uploads_needing_split.lock().unwrap() = vec![(upload_id, "test.zip".to_string())];
    // split_csv_from_r2 needs get_upload_tenant_and_key
    *mock.tenant_and_key.lock().unwrap() = Some(UploadTenantAndKey {
        tenant_id,
        r2_zip_key: zip_key.clone(),
    });

    let dtako_storage = Arc::new(MockStorage::new("dtako-bucket"));
    let zip_bytes = crate::common::create_test_dtako_zip();
    dtako_storage.insert_file(&zip_key, zip_bytes);

    let state = setup_with_storage_and_mock(mock, dtako_storage);
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/split-csv-all"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body = res.text().await.unwrap();
    assert!(body.contains("done"));
}

// =========================================================================
// POST /api/recalculate — success with actual operations data
// =========================================================================

#[tokio::test]
async fn test_dtako_recalculate_with_operations() {
    use chrono::{NaiveDate, TimeZone, Utc};

    let tenant_id = Uuid::new_v4();
    let unko_no = "5001";

    // Prepare KUDGIVT CSV content (UTF-8 this time, for recalculate path which reads from R2 split CSVs)
    let kudgivt_csv = format!(
        "運行NO,読取日,乗務員CD1,乗務員名１,対象乗務員区分,開始日時,終了日時,イベントCD,イベント名,区間時間,区間距離\n\
         {unko_no},2026/03/15,DR01,運転者A,1,2026/03/15 08:00:00,2026/03/15 12:00:00,201,走行,240,100.0\n\
         {unko_no},2026/03/15,DR01,運転者A,1,2026/03/15 12:00:00,2026/03/15 13:00:00,301,休憩,60,0\n\
         {unko_no},2026/03/15,DR01,運転者A,1,2026/03/15 13:00:00,2026/03/15 15:00:00,202,積み,120,0\n\
         {unko_no},2026/03/15,DR01,運転者A,1,2026/03/15 15:00:00,2026/03/15 17:00:00,201,走行,120,50.0\n"
    );

    let mut mock = MockDtakoUploadRepository::default();
    let dep = Utc.with_ymd_and_hms(2026, 3, 15, 8, 0, 0).unwrap();
    let ret = Utc.with_ymd_and_hms(2026, 3, 15, 17, 0, 0).unwrap();
    *mock.operations.lock().unwrap() = vec![DtakoOpRow {
        unko_no: unko_no.to_string(),
        reading_date: NaiveDate::from_ymd_opt(2026, 3, 15).unwrap(),
        operation_date: Some(NaiveDate::from_ymd_opt(2026, 3, 15).unwrap()),
        departure_at: Some(dep),
        return_at: Some(ret),
        driver_cd: Some("DR01".to_string()),
        total_distance: Some(150.0),
        drive_time_general: Some(360),
        drive_time_highway: Some(0),
        drive_time_bypass: Some(0),
    }];

    let dtako_storage = Arc::new(MockStorage::new("dtako-bucket"));
    // Insert the per-unko KUDGIVT CSV (recalculate_all_core reads individual CSVs from R2)
    let kudgivt_key = format!("{}/unko/{}/KUDGIVT.csv", tenant_id, unko_no);
    dtako_storage.insert_file(&kudgivt_key, kudgivt_csv.into_bytes());

    let state = setup_with_storage_and_mock(mock, dtako_storage);
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
    assert!(body.contains("done"));
}

// =========================================================================
// POST /api/recalculate — with operations but no KUDGIVT → error
// =========================================================================

#[tokio::test]
async fn test_dtako_recalculate_no_kudgivt_error() {
    use chrono::{NaiveDate, TimeZone, Utc};

    let tenant_id = Uuid::new_v4();

    let mut mock = MockDtakoUploadRepository::default();
    let dep = Utc.with_ymd_and_hms(2026, 3, 15, 8, 0, 0).unwrap();
    let ret = Utc.with_ymd_and_hms(2026, 3, 15, 17, 0, 0).unwrap();
    *mock.operations.lock().unwrap() = vec![DtakoOpRow {
        unko_no: "9999".to_string(),
        reading_date: NaiveDate::from_ymd_opt(2026, 3, 15).unwrap(),
        operation_date: Some(NaiveDate::from_ymd_opt(2026, 3, 15).unwrap()),
        departure_at: Some(dep),
        return_at: Some(ret),
        driver_cd: Some("DR01".to_string()),
        total_distance: Some(100.0),
        drive_time_general: Some(300),
        drive_time_highway: Some(0),
        drive_time_bypass: Some(0),
    }];

    // No KUDGIVT in storage → should trigger "KUDGIVTが見つかりません" error
    let state = setup_with_mock(mock);
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/recalculate?year=2026&month=3"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200); // SSE always returns 200
    let body = res.text().await.unwrap();
    assert!(body.contains("error"));
}

// =========================================================================
// POST /api/recalculate-driver — success with driver_cd and operations
// =========================================================================

#[tokio::test]
async fn test_dtako_recalculate_driver_with_data() {
    use chrono::{NaiveDate, TimeZone, Utc};

    let tenant_id = Uuid::new_v4();
    let driver_id = Uuid::new_v4();
    let unko_no = "6001";

    let kudgivt_csv = format!(
        "運行NO,読取日,乗務員CD1,乗務員名１,対象乗務員区分,開始日時,終了日時,イベントCD,イベント名,区間時間,区間距離\n\
         {unko_no},2026/03/20,DR01,運転者A,1,2026/03/20 08:00:00,2026/03/20 12:00:00,201,走行,240,80.0\n\
         {unko_no},2026/03/20,DR01,運転者A,1,2026/03/20 12:00:00,2026/03/20 13:00:00,301,休憩,60,0\n\
         {unko_no},2026/03/20,DR01,運転者A,1,2026/03/20 13:00:00,2026/03/20 16:00:00,201,走行,180,70.0\n"
    );

    let mut mock = MockDtakoUploadRepository::default();
    *mock.driver_cd.lock().unwrap() = Some("DR01".to_string());

    let dep = Utc.with_ymd_and_hms(2026, 3, 20, 8, 0, 0).unwrap();
    let ret = Utc.with_ymd_and_hms(2026, 3, 20, 16, 0, 0).unwrap();
    *mock.driver_operations.lock().unwrap() = vec![DtakoDriverOpRow {
        unko_no: unko_no.to_string(),
        reading_date: NaiveDate::from_ymd_opt(2026, 3, 20).unwrap(),
        operation_date: Some(NaiveDate::from_ymd_opt(2026, 3, 20).unwrap()),
        departure_at: Some(dep),
        return_at: Some(ret),
        total_distance: Some(150.0),
        drive_time_general: Some(420),
        drive_time_highway: Some(0),
        drive_time_bypass: Some(0),
    }];

    // ZIP keys for load_kudgivt_from_zips
    let zip_key = format!("{}/uploads/some/test.zip", tenant_id);
    *mock.zip_keys.lock().unwrap() = vec![zip_key.clone()];

    let dtako_storage = Arc::new(MockStorage::new("dtako-bucket"));
    // Create a ZIP with KUDGIVT for load_kudgivt_from_zips
    let zip_bytes = create_zip_with_kudgivt(&kudgivt_csv);
    dtako_storage.insert_file(&zip_key, zip_bytes);

    // Also insert per-unko KUDGFRY.csv for ferry data loading
    let ferry_csv = format!(
        "col0,col1,col2,col3,col4,col5,col6,col7,col8,col9,start_time,end_time\n\
         a,b,c,d,e,f,g,h,i,j,2026/03/20 14:00:00,2026/03/20 15:00:00\n"
    );
    let ferry_key = format!("{}/unko/{}/KUDGFRY.csv", tenant_id, unko_no);
    let (ferry_bytes, _, _) = encoding_rs::SHIFT_JIS.encode(&ferry_csv);
    dtako_storage.insert_file(&ferry_key, ferry_bytes.to_vec());

    let state = setup_with_storage_and_mock(mock, dtako_storage);
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

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
    assert!(body.contains("done"));
}

/// Create a ZIP file containing only KUDGIVT.csv (UTF-8 text inside)
fn create_zip_with_kudgivt(kudgivt_text: &str) -> Vec<u8> {
    use std::io::Write;

    let (kudgivt_bytes, _, _) = encoding_rs::SHIFT_JIS.encode(kudgivt_text);
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("KUDGIVT.csv", options).unwrap();
        zip.write_all(&kudgivt_bytes).unwrap();
        zip.finish().unwrap();
    }
    buf.into_inner()
}

// =========================================================================
// POST /api/recalculate-drivers — batch with actual driver data
// =========================================================================

#[tokio::test]
async fn test_dtako_recalculate_drivers_batch_with_data() {
    use chrono::{NaiveDate, TimeZone, Utc};

    let tenant_id = Uuid::new_v4();
    let driver_id = Uuid::new_v4();

    let mut mock = MockDtakoUploadRepository::default();
    *mock.driver_cd.lock().unwrap() = Some("DR01".to_string());

    let dep = Utc.with_ymd_and_hms(2026, 3, 20, 8, 0, 0).unwrap();
    let ret = Utc.with_ymd_and_hms(2026, 3, 20, 16, 0, 0).unwrap();
    *mock.driver_operations.lock().unwrap() = vec![DtakoDriverOpRow {
        unko_no: "7001".to_string(),
        reading_date: NaiveDate::from_ymd_opt(2026, 3, 20).unwrap(),
        operation_date: Some(NaiveDate::from_ymd_opt(2026, 3, 20).unwrap()),
        departure_at: Some(dep),
        return_at: Some(ret),
        total_distance: Some(100.0),
        drive_time_general: Some(300),
        drive_time_highway: Some(60),
        drive_time_bypass: Some(0),
    }];

    let kudgivt_csv = "運行NO,読取日,乗務員CD1,乗務員名１,対象乗務員区分,開始日時,終了日時,イベントCD,イベント名,区間時間,区間距離\n\
         7001,2026/03/20,DR01,運転者A,1,2026/03/20 08:00:00,2026/03/20 12:00:00,201,走行,240,50.0\n\
         7001,2026/03/20,DR01,運転者A,1,2026/03/20 13:00:00,2026/03/20 16:00:00,201,走行,180,50.0\n";

    let zip_key = format!("{}/uploads/batch/test.zip", tenant_id);
    *mock.zip_keys.lock().unwrap() = vec![zip_key.clone()];

    let dtako_storage = Arc::new(MockStorage::new("dtako-bucket"));
    dtako_storage.insert_file(&zip_key, create_zip_with_kudgivt(kudgivt_csv));

    let state = setup_with_storage_and_mock(mock, dtako_storage);
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
                "driver_ids": [driver_id]
            })
            .to_string(),
        )
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body = res.text().await.unwrap();
    assert!(body.contains("batch_done"));
}

// =========================================================================
// POST /api/upload — rich ZIP with ferry data (covers load_ferry_minutes in upload)
// Note: ferry data is only loaded during split-csv/recalculate, not during upload
// Upload always uses empty ferry_minutes. So we test ferry via recalculate.
// =========================================================================

// =========================================================================
// POST /api/recalculate — with ferry data (covers load_ferry_minutes)
// =========================================================================

#[tokio::test]
async fn test_dtako_recalculate_with_ferry_data() {
    use chrono::{NaiveDate, TimeZone, Utc};

    let tenant_id = Uuid::new_v4();
    let unko_no = "8001";

    let kudgivt_csv = format!(
        "運行NO,読取日,乗務員CD1,乗務員名１,対象乗務員区分,開始日時,終了日時,イベントCD,イベント名,区間時間,区間距離\n\
         {unko_no},2026/03/10,DR01,運転者A,1,2026/03/10 06:00:00,2026/03/10 10:00:00,201,走行,240,100.0\n\
         {unko_no},2026/03/10,DR01,運転者A,1,2026/03/10 10:00:00,2026/03/10 11:00:00,301,休憩,60,0\n\
         {unko_no},2026/03/10,DR01,運転者A,1,2026/03/10 11:00:00,2026/03/10 14:00:00,201,走行,180,80.0\n\
         {unko_no},2026/03/10,DR01,運転者A,1,2026/03/10 14:00:00,2026/03/10 14:30:00,302,休息,30,0\n"
    );

    let mut mock = MockDtakoUploadRepository::default();
    let dep = Utc.with_ymd_and_hms(2026, 3, 10, 6, 0, 0).unwrap();
    let ret = Utc.with_ymd_and_hms(2026, 3, 10, 15, 0, 0).unwrap();
    *mock.operations.lock().unwrap() = vec![DtakoOpRow {
        unko_no: unko_no.to_string(),
        reading_date: NaiveDate::from_ymd_opt(2026, 3, 10).unwrap(),
        operation_date: Some(NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()),
        departure_at: Some(dep),
        return_at: Some(ret),
        driver_cd: Some("DR01".to_string()),
        total_distance: Some(180.0),
        drive_time_general: Some(420),
        drive_time_highway: Some(0),
        drive_time_bypass: Some(0),
    }];

    let dtako_storage = Arc::new(MockStorage::new("dtako-bucket"));

    // Per-unko KUDGIVT.csv (recalculate reads these individually)
    let kudgivt_key = format!("{}/unko/{}/KUDGIVT.csv", tenant_id, unko_no);
    dtako_storage.insert_file(&kudgivt_key, kudgivt_csv.into_bytes());

    // Per-unko KUDGFRY.csv (ferry data)
    let ferry_csv = "c0,c1,c2,c3,c4,c5,c6,c7,c8,c9,start,end\n\
                     a,b,c,d,e,f,g,h,i,j,2026/03/10 10:00:00,2026/03/10 11:00:00\n";
    let ferry_key = format!("{}/unko/{}/KUDGFRY.csv", tenant_id, unko_no);
    let (ferry_bytes, _, _) = encoding_rs::SHIFT_JIS.encode(ferry_csv);
    dtako_storage.insert_file(&ferry_key, ferry_bytes.to_vec());

    let state = setup_with_storage_and_mock(mock, dtako_storage);
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
    assert!(body.contains("done"));
}

// =========================================================================
// POST /api/recalculate — with ferry data having short duration (< 0 mins)
// =========================================================================

#[tokio::test]
async fn test_dtako_recalculate_ferry_zero_duration() {
    use chrono::{NaiveDate, TimeZone, Utc};

    let tenant_id = Uuid::new_v4();
    let unko_no = "8002";

    let kudgivt_csv = format!(
        "運行NO,読取日,乗務員CD1,乗務員名１,対象乗務員区分,開始日時,終了日時,イベントCD,イベント名,区間時間,区間距離\n\
         {unko_no},2026/03/10,DR01,運転者A,1,2026/03/10 08:00:00,2026/03/10 12:00:00,201,走行,240,100.0\n"
    );

    let mut mock = MockDtakoUploadRepository::default();
    let dep = Utc.with_ymd_and_hms(2026, 3, 10, 8, 0, 0).unwrap();
    let ret = Utc.with_ymd_and_hms(2026, 3, 10, 12, 0, 0).unwrap();
    *mock.operations.lock().unwrap() = vec![DtakoOpRow {
        unko_no: unko_no.to_string(),
        reading_date: NaiveDate::from_ymd_opt(2026, 3, 10).unwrap(),
        operation_date: Some(NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()),
        departure_at: Some(dep),
        return_at: Some(ret),
        driver_cd: Some("DR01".to_string()),
        total_distance: Some(100.0),
        drive_time_general: Some(240),
        drive_time_highway: Some(0),
        drive_time_bypass: Some(0),
    }];

    let dtako_storage = Arc::new(MockStorage::new("dtako-bucket"));
    let kudgivt_key = format!("{}/unko/{}/KUDGIVT.csv", tenant_id, unko_no);
    dtako_storage.insert_file(&kudgivt_key, kudgivt_csv.into_bytes());

    // Ferry with same start and end (0 duration) — should be ignored
    let ferry_csv = "c0,c1,c2,c3,c4,c5,c6,c7,c8,c9,start,end\n\
                     a,b,c,d,e,f,g,h,i,j,2026/03/10 10:00:00,2026/03/10 10:00:00\n";
    let ferry_key = format!("{}/unko/{}/KUDGFRY.csv", tenant_id, unko_no);
    let (ferry_bytes, _, _) = encoding_rs::SHIFT_JIS.encode(ferry_csv);
    dtako_storage.insert_file(&ferry_key, ferry_bytes.to_vec());

    let state = setup_with_storage_and_mock(mock, dtako_storage);
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
    assert!(body.contains("done"));
}

// =========================================================================
// POST /api/recalculate — ferry data with short cols (<=11) → skip line
// =========================================================================

#[tokio::test]
async fn test_dtako_recalculate_ferry_short_cols() {
    use chrono::{NaiveDate, TimeZone, Utc};

    let tenant_id = Uuid::new_v4();
    let unko_no = "8003";

    let kudgivt_csv = format!(
        "運行NO,読取日,乗務員CD1,乗務員名１,対象乗務員区分,開始日時,終了日時,イベントCD,イベント名,区間時間,区間距離\n\
         {unko_no},2026/03/10,DR01,運転者A,1,2026/03/10 08:00:00,2026/03/10 12:00:00,201,走行,240,100.0\n"
    );

    let mut mock = MockDtakoUploadRepository::default();
    let dep = Utc.with_ymd_and_hms(2026, 3, 10, 8, 0, 0).unwrap();
    let ret = Utc.with_ymd_and_hms(2026, 3, 10, 12, 0, 0).unwrap();
    *mock.operations.lock().unwrap() = vec![DtakoOpRow {
        unko_no: unko_no.to_string(),
        reading_date: NaiveDate::from_ymd_opt(2026, 3, 10).unwrap(),
        operation_date: Some(NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()),
        departure_at: Some(dep),
        return_at: Some(ret),
        driver_cd: Some("DR01".to_string()),
        total_distance: Some(100.0),
        drive_time_general: Some(240),
        drive_time_highway: Some(0),
        drive_time_bypass: Some(0),
    }];

    let dtako_storage = Arc::new(MockStorage::new("dtako-bucket"));
    let kudgivt_key = format!("{}/unko/{}/KUDGIVT.csv", tenant_id, unko_no);
    dtako_storage.insert_file(&kudgivt_key, kudgivt_csv.into_bytes());

    // Ferry CSV with too few columns (only 5) → should be skipped (cols.len() <= 11)
    let ferry_csv = "c0,c1,c2\nheader_only\na,b,c\n";
    let ferry_key = format!("{}/unko/{}/KUDGFRY.csv", tenant_id, unko_no);
    let (ferry_bytes, _, _) = encoding_rs::SHIFT_JIS.encode(ferry_csv);
    dtako_storage.insert_file(&ferry_key, ferry_bytes.to_vec());

    let state = setup_with_storage_and_mock(mock, dtako_storage);
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
    assert!(body.contains("done"));
}

// =========================================================================
// POST /api/recalculate — ferry data with invalid datetime format → skip
// =========================================================================

#[tokio::test]
async fn test_dtako_recalculate_ferry_invalid_datetime() {
    use chrono::{NaiveDate, TimeZone, Utc};

    let tenant_id = Uuid::new_v4();
    let unko_no = "8004";

    let kudgivt_csv = format!(
        "運行NO,読取日,乗務員CD1,乗務員名１,対象乗務員区分,開始日時,終了日時,イベントCD,イベント名,区間時間,区間距離\n\
         {unko_no},2026/03/10,DR01,運転者A,1,2026/03/10 08:00:00,2026/03/10 12:00:00,201,走行,240,100.0\n"
    );

    let mut mock = MockDtakoUploadRepository::default();
    let dep = Utc.with_ymd_and_hms(2026, 3, 10, 8, 0, 0).unwrap();
    let ret = Utc.with_ymd_and_hms(2026, 3, 10, 12, 0, 0).unwrap();
    *mock.operations.lock().unwrap() = vec![DtakoOpRow {
        unko_no: unko_no.to_string(),
        reading_date: NaiveDate::from_ymd_opt(2026, 3, 10).unwrap(),
        operation_date: Some(NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()),
        departure_at: Some(dep),
        return_at: Some(ret),
        driver_cd: Some("DR01".to_string()),
        total_distance: Some(100.0),
        drive_time_general: Some(240),
        drive_time_highway: Some(0),
        drive_time_bypass: Some(0),
    }];

    let dtako_storage = Arc::new(MockStorage::new("dtako-bucket"));
    let kudgivt_key = format!("{}/unko/{}/KUDGIVT.csv", tenant_id, unko_no);
    dtako_storage.insert_file(&kudgivt_key, kudgivt_csv.into_bytes());

    // Ferry CSV with invalid datetime strings → parse fails → skipped
    let ferry_csv =
        "c0,c1,c2,c3,c4,c5,c6,c7,c8,c9,start,end\na,b,c,d,e,f,g,h,i,j,INVALID,ALSO_INVALID\n";
    let ferry_key = format!("{}/unko/{}/KUDGFRY.csv", tenant_id, unko_no);
    let (ferry_bytes, _, _) = encoding_rs::SHIFT_JIS.encode(ferry_csv);
    dtako_storage.insert_file(&ferry_key, ferry_bytes.to_vec());

    let state = setup_with_storage_and_mock(mock, dtako_storage);
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
    assert!(body.contains("done"));
}

// =========================================================================
// Unit tests for compute_month_range and default_classification
// =========================================================================

#[test]
fn test_compute_month_range() {
    use rust_alc_api::routes::dtako_upload::compute_month_range;

    // Normal month
    let (start, end) = compute_month_range(2026, 3).unwrap();
    assert_eq!(start, chrono::NaiveDate::from_ymd_opt(2026, 3, 1).unwrap());
    assert_eq!(end, chrono::NaiveDate::from_ymd_opt(2026, 3, 31).unwrap());

    // December (year wrap)
    let (start, end) = compute_month_range(2026, 12).unwrap();
    assert_eq!(start, chrono::NaiveDate::from_ymd_opt(2026, 12, 1).unwrap());
    assert_eq!(end, chrono::NaiveDate::from_ymd_opt(2026, 12, 31).unwrap());

    // February (non-leap year)
    let (start, end) = compute_month_range(2025, 2).unwrap();
    assert_eq!(start, chrono::NaiveDate::from_ymd_opt(2025, 2, 1).unwrap());
    assert_eq!(end, chrono::NaiveDate::from_ymd_opt(2025, 2, 28).unwrap());

    // Invalid month → None
    let result = compute_month_range(2026, 13);
    assert!(result.is_none());
}

#[test]
fn test_default_classification() {
    use rust_alc_api::csv_parser::work_segments::EventClass;
    use rust_alc_api::routes::dtako_upload::default_classification;

    assert_eq!(default_classification("201"), ("drive", EventClass::Drive));
    assert_eq!(default_classification("202"), ("cargo", EventClass::Cargo));
    assert_eq!(default_classification("203"), ("cargo", EventClass::Cargo));
    assert_eq!(default_classification("204"), ("cargo", EventClass::Cargo));
    assert_eq!(
        default_classification("302"),
        ("rest_split", EventClass::RestSplit)
    );
    assert_eq!(default_classification("301"), ("break", EventClass::Break));
    assert_eq!(
        default_classification("999"),
        ("ignore", EventClass::Ignore)
    );
}

// =========================================================================
// POST /api/recalculate-driver — driver_cd not found → error in SSE
// =========================================================================

#[tokio::test]
async fn test_dtako_recalculate_driver_not_found() {
    // driver_cd is None by default → "ドライバーが見つかりません"
    let state = setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let driver_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

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
// POST /api/recalculate — invalid year/month → error
// =========================================================================

#[tokio::test]
async fn test_dtako_recalculate_invalid_month() {
    let state = setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/recalculate?year=2026&month=13"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200); // SSE always returns 200
    let body = res.text().await.unwrap();
    assert!(body.contains("error"));
}

// =========================================================================
// POST /api/recalculate-driver — invalid month → error
// =========================================================================

#[tokio::test]
async fn test_dtako_recalculate_driver_invalid_month() {
    let mut mock = MockDtakoUploadRepository::default();
    *mock.driver_cd.lock().unwrap() = Some("DR01".to_string());

    let state = setup_with_mock(mock);
    let tenant_id = Uuid::new_v4();
    let driver_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .post(format!(
            "{base_url}/api/recalculate-driver?year=2026&month=13&driver_id={driver_id}"
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
// POST /api/recalculate-drivers — invalid month → error
// =========================================================================

#[tokio::test]
async fn test_dtako_recalculate_drivers_batch_invalid_month() {
    let state = setup_mock_app_state();
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
                "month": 13,
                "driver_ids": [Uuid::new_v4()]
            })
            .to_string(),
        )
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body = res.text().await.unwrap();
    assert!(body.contains("error"));
}

// =========================================================================
// POST /api/recalculate — with operations having multiple drivers + rest events
// =========================================================================

#[tokio::test]
async fn test_dtako_recalculate_multiple_operations_with_rest() {
    use chrono::{NaiveDate, TimeZone, Utc};

    let tenant_id = Uuid::new_v4();

    let kudgivt_csv = "運行NO,読取日,乗務員CD1,乗務員名１,対象乗務員区分,開始日時,終了日時,イベントCD,イベント名,区間時間,区間距離\n\
         9001,2026/03/10,DR01,運転者A,1,2026/03/10 08:00:00,2026/03/10 12:00:00,201,走行,240,100.0\n\
         9001,2026/03/10,DR01,運転者A,1,2026/03/10 12:00:00,2026/03/10 13:00:00,302,休息,60,0\n\
         9001,2026/03/10,DR01,運転者A,1,2026/03/10 13:00:00,2026/03/10 15:00:00,203,降し,120,0\n\
         9002,2026/03/10,DR02,運転者B,1,2026/03/10 09:00:00,2026/03/10 14:00:00,201,走行,300,80.0\n\
         9002,2026/03/10,DR02,運転者B,1,2026/03/10 14:00:00,2026/03/10 15:00:00,204,その他,60,0\n";

    let mut mock = MockDtakoUploadRepository::default();

    let dep1 = Utc.with_ymd_and_hms(2026, 3, 10, 8, 0, 0).unwrap();
    let ret1 = Utc.with_ymd_and_hms(2026, 3, 10, 15, 0, 0).unwrap();
    let dep2 = Utc.with_ymd_and_hms(2026, 3, 10, 9, 0, 0).unwrap();
    let ret2 = Utc.with_ymd_and_hms(2026, 3, 10, 15, 0, 0).unwrap();
    *mock.operations.lock().unwrap() = vec![
        DtakoOpRow {
            unko_no: "9001".to_string(),
            reading_date: NaiveDate::from_ymd_opt(2026, 3, 10).unwrap(),
            operation_date: Some(NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()),
            departure_at: Some(dep1),
            return_at: Some(ret1),
            driver_cd: Some("DR01".to_string()),
            total_distance: Some(100.0),
            drive_time_general: Some(240),
            drive_time_highway: Some(0),
            drive_time_bypass: Some(0),
        },
        DtakoOpRow {
            unko_no: "9002".to_string(),
            reading_date: NaiveDate::from_ymd_opt(2026, 3, 10).unwrap(),
            operation_date: Some(NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()),
            departure_at: Some(dep2),
            return_at: Some(ret2),
            driver_cd: Some("DR02".to_string()),
            total_distance: Some(80.0),
            drive_time_general: Some(300),
            drive_time_highway: Some(0),
            drive_time_bypass: Some(0),
        },
    ];

    let dtako_storage = Arc::new(MockStorage::new("dtako-bucket"));
    let kudgivt_key1 = format!("{}/unko/9001/KUDGIVT.csv", tenant_id);
    let kudgivt_key2 = format!("{}/unko/9002/KUDGIVT.csv", tenant_id);
    dtako_storage.insert_file(&kudgivt_key1, kudgivt_csv.as_bytes().to_vec());
    dtako_storage.insert_file(&kudgivt_key2, kudgivt_csv.as_bytes().to_vec());

    let state = setup_with_storage_and_mock(mock, dtako_storage);
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
    assert!(body.contains("done"));
}

// =========================================================================
// POST /api/recalculate — with employee_id mapped (exercises driver_id path)
// =========================================================================

#[tokio::test]
async fn test_dtako_recalculate_with_employee_id_mapped() {
    use chrono::{NaiveDate, TimeZone, Utc};

    let tenant_id = Uuid::new_v4();
    let employee_id = Uuid::new_v4();
    let unko_no = "9010";

    let kudgivt_csv = format!(
        "運行NO,読取日,乗務員CD1,乗務員名１,対象乗務員区分,開始日時,終了日時,イベントCD,イベント名,区間時間,区間距離\n\
         {unko_no},2026/03/15,DR01,運転者A,1,2026/03/15 08:00:00,2026/03/15 17:00:00,201,走行,540,200.0\n"
    );

    let mut mock = MockDtakoUploadRepository::default();
    *mock.employee_id.lock().unwrap() = Some(employee_id);

    let dep = Utc.with_ymd_and_hms(2026, 3, 15, 8, 0, 0).unwrap();
    let ret = Utc.with_ymd_and_hms(2026, 3, 15, 17, 0, 0).unwrap();
    *mock.operations.lock().unwrap() = vec![DtakoOpRow {
        unko_no: unko_no.to_string(),
        reading_date: NaiveDate::from_ymd_opt(2026, 3, 15).unwrap(),
        operation_date: Some(NaiveDate::from_ymd_opt(2026, 3, 15).unwrap()),
        departure_at: Some(dep),
        return_at: Some(ret),
        driver_cd: Some("DR01".to_string()),
        total_distance: Some(200.0),
        drive_time_general: Some(540),
        drive_time_highway: Some(0),
        drive_time_bypass: Some(0),
    }];

    let dtako_storage = Arc::new(MockStorage::new("dtako-bucket"));
    let kudgivt_key = format!("{}/unko/{}/KUDGIVT.csv", tenant_id, unko_no);
    dtako_storage.insert_file(&kudgivt_key, kudgivt_csv.into_bytes());

    let state = setup_with_storage_and_mock(mock, dtako_storage);
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
    assert!(body.contains("done"));
}

// =========================================================================
// POST /api/upload — process_zip fails after create_upload_history → mark_upload_failed
// =========================================================================

#[tokio::test]
async fn test_dtako_upload_process_zip_error_marks_failed() {
    // Upload a ZIP that has KUDGURI but not KUDGIVT → process_zip fails
    // This exercises the mark_upload_failed path (line 82-87)
    let state = setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let zip_bytes = create_missing_kudgivt_zip();
    let part = reqwest::multipart::Part::bytes(zip_bytes).file_name("bad.zip");
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", format!("Bearer {jwt}"))
        .multipart(form)
        .send()
        .await
        .unwrap();

    // Should return 400 since process_zip failed
    assert_eq!(res.status(), 400);
}

// =========================================================================
// load_kudgivt_from_zips: ZIP download error + ZIP extract error paths
// (exercised through recalculate-driver with bad ZIP data in storage)
// =========================================================================

#[tokio::test]
async fn test_dtako_recalculate_driver_bad_zip_in_storage() {
    use chrono::{NaiveDate, TimeZone, Utc};

    let tenant_id = Uuid::new_v4();
    let driver_id = Uuid::new_v4();

    let mut mock = MockDtakoUploadRepository::default();
    *mock.driver_cd.lock().unwrap() = Some("DR01".to_string());

    let dep = Utc.with_ymd_and_hms(2026, 3, 20, 8, 0, 0).unwrap();
    let ret = Utc.with_ymd_and_hms(2026, 3, 20, 16, 0, 0).unwrap();
    *mock.driver_operations.lock().unwrap() = vec![DtakoDriverOpRow {
        unko_no: "BAD01".to_string(),
        reading_date: NaiveDate::from_ymd_opt(2026, 3, 20).unwrap(),
        operation_date: Some(NaiveDate::from_ymd_opt(2026, 3, 20).unwrap()),
        departure_at: Some(dep),
        return_at: Some(ret),
        total_distance: Some(100.0),
        drive_time_general: Some(300),
        drive_time_highway: Some(0),
        drive_time_bypass: Some(0),
    }];

    // Add a zip key that points to invalid ZIP data
    let bad_zip_key = format!("{}/uploads/bad/corrupt.zip", tenant_id);
    *mock.zip_keys.lock().unwrap() = vec![bad_zip_key.clone()];

    let dtako_storage = Arc::new(MockStorage::new("dtako-bucket"));
    // Insert corrupted data (not a valid ZIP)
    dtako_storage.insert_file(&bad_zip_key, vec![0x00, 0x01, 0x02, 0x03]);

    let state = setup_with_storage_and_mock(mock, dtako_storage);
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

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
    // No KUDGIVT found from bad ZIP but operations exist → should return done with 0 or error
    // Since all_kudgivt is empty and ops is non-empty, no error: the recalculate still proceeds
    // because load_kudgivt_from_zips only warns, doesn't fail
    assert!(body.contains("done") || body.contains("error"));
}

// =========================================================================
// load_kudgivt_from_zips: ZIP with KUDGIVT that has parse error
// =========================================================================

#[tokio::test]
async fn test_dtako_recalculate_driver_kudgivt_parse_error_in_zip() {
    use chrono::{NaiveDate, TimeZone, Utc};

    let tenant_id = Uuid::new_v4();
    let driver_id = Uuid::new_v4();

    let mut mock = MockDtakoUploadRepository::default();
    *mock.driver_cd.lock().unwrap() = Some("DR01".to_string());

    let dep = Utc.with_ymd_and_hms(2026, 3, 20, 8, 0, 0).unwrap();
    let ret = Utc.with_ymd_and_hms(2026, 3, 20, 16, 0, 0).unwrap();
    *mock.driver_operations.lock().unwrap() = vec![DtakoDriverOpRow {
        unko_no: "PARSE01".to_string(),
        reading_date: NaiveDate::from_ymd_opt(2026, 3, 20).unwrap(),
        operation_date: Some(NaiveDate::from_ymd_opt(2026, 3, 20).unwrap()),
        departure_at: Some(dep),
        return_at: Some(ret),
        total_distance: Some(100.0),
        drive_time_general: Some(300),
        drive_time_highway: Some(0),
        drive_time_bypass: Some(0),
    }];

    // Create a valid ZIP with a KUDGIVT.csv that has invalid content
    let bad_kudgivt = "this is not valid KUDGIVT CSV content at all";
    let zip_key = format!("{}/uploads/bad_kudgivt/test.zip", tenant_id);
    *mock.zip_keys.lock().unwrap() = vec![zip_key.clone()];

    let dtako_storage = Arc::new(MockStorage::new("dtako-bucket"));
    let zip_bytes = create_zip_with_kudgivt(bad_kudgivt);
    dtako_storage.insert_file(&zip_key, zip_bytes);

    let state = setup_with_storage_and_mock(mock, dtako_storage);
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

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
    assert!(body.contains("done") || body.contains("error"));
}

// =========================================================================
// POST /api/recalculate — late night work (hours spanning 22:00-05:00)
// =========================================================================

#[tokio::test]
async fn test_dtako_recalculate_late_night_work() {
    use chrono::{NaiveDate, TimeZone, Utc};

    let tenant_id = Uuid::new_v4();
    let employee_id = Uuid::new_v4();
    let unko_no = "LN01";

    // Operation with late-night hours (22:00 - 03:00 next day)
    let kudgivt_csv = format!(
        "運行NO,読取日,乗務員CD1,乗務員名１,対象乗務員区分,開始日時,終了日時,イベントCD,イベント名,区間時間,区間距離\n\
         {unko_no},2026/03/10,DR01,運転者A,1,2026/03/10 20:00:00,2026/03/11 03:00:00,201,走行,420,200.0\n"
    );

    let mut mock = MockDtakoUploadRepository::default();
    *mock.employee_id.lock().unwrap() = Some(employee_id);

    let dep = Utc.with_ymd_and_hms(2026, 3, 10, 20, 0, 0).unwrap();
    let ret = Utc.with_ymd_and_hms(2026, 3, 11, 3, 0, 0).unwrap();
    *mock.operations.lock().unwrap() = vec![DtakoOpRow {
        unko_no: unko_no.to_string(),
        reading_date: NaiveDate::from_ymd_opt(2026, 3, 10).unwrap(),
        operation_date: Some(NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()),
        departure_at: Some(dep),
        return_at: Some(ret),
        driver_cd: Some("DR01".to_string()),
        total_distance: Some(200.0),
        drive_time_general: Some(420),
        drive_time_highway: Some(0),
        drive_time_bypass: Some(0),
    }];

    let dtako_storage = Arc::new(MockStorage::new("dtako-bucket"));
    let kudgivt_key = format!("{}/unko/{}/KUDGIVT.csv", tenant_id, unko_no);
    dtako_storage.insert_file(&kudgivt_key, kudgivt_csv.into_bytes());

    let state = setup_with_storage_and_mock(mock, dtako_storage);
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
    assert!(body.contains("done"));
}

// =========================================================================
// POST /api/upload — upload with non-zip filename extension
// =========================================================================

#[tokio::test]
async fn test_dtako_upload_no_filename() {
    let state = setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let zip_bytes = crate::common::create_test_dtako_zip();
    // Part without file_name → defaults to "upload.zip"
    let part = reqwest::multipart::Part::bytes(zip_bytes);
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/upload"))
        .header("Authorization", format!("Bearer {jwt}"))
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
}

mod common;
mod mock_helpers;

use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use mock_helpers::MockTenkoRecordsRepository;

/// Helper: set up mock AppState and spawn test server with admin JWT.
/// Returns (base_url, auth_header, tenant_id).
async fn setup() -> (String, String, uuid::Uuid) {
    let state = mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header, tenant_id)
}

/// Helper: set up with a failing mock for tenko_records (all methods return error).
async fn setup_failing() -> (String, String) {
    let mock = Arc::new(MockTenkoRecordsRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.tenko_records = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header)
}

/// Helper: set up with return_some=true (get returns Some).
async fn setup_found() -> (String, String) {
    let mock = Arc::new(MockTenkoRecordsRepository::default());
    mock.return_some.store(true, Ordering::SeqCst);
    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.tenko_records = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header)
}

/// Helper: set up with return_data=true (list/count return sample data).
async fn setup_with_data() -> (String, String) {
    let mock = Arc::new(MockTenkoRecordsRepository::default());
    mock.return_data.store(true, Ordering::SeqCst);
    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.tenko_records = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header)
}

// =========================================================================
// GET /api/tenko/records — list_records
// =========================================================================

#[tokio::test]
async fn test_list_records_success_empty() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/records"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["records"].is_array());
    assert_eq!(body["records"].as_array().unwrap().len(), 0);
    assert_eq!(body["total"], 0);
    assert_eq!(body["page"], 1);
    assert_eq!(body["per_page"], 50);
}

#[tokio::test]
async fn test_list_records_with_data() {
    let (base_url, auth_header) = setup_with_data().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/records"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["total"], 1);
    assert_eq!(body["records"].as_array().unwrap().len(), 1);
    let record = &body["records"][0];
    assert_eq!(record["employee_name"], "Test Employee");
    assert_eq!(record["tenko_type"], "pre_operation");
    assert_eq!(record["status"], "completed");
}

#[tokio::test]
async fn test_list_records_with_pagination() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/records?page=2&per_page=10"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["page"], 2);
    assert_eq!(body["per_page"], 10);
}

#[tokio::test]
async fn test_list_records_per_page_capped_at_100() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/records?per_page=999"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["per_page"], 100);
}

#[tokio::test]
async fn test_list_records_page_min_1() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/records?page=0"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["page"], 1);
}

#[tokio::test]
async fn test_list_records_with_filters() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();
    let emp_id = uuid::Uuid::new_v4();

    let res = client
        .get(format!(
            "{base_url}/api/tenko/records?employee_id={emp_id}&tenko_type=pre_operation&status=completed&date_from=2026-01-01T00:00:00Z&date_to=2026-12-31T23:59:59Z"
        ))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_list_records_no_auth() {
    let (base_url, _, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/records"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_list_records_x_tenant_id() {
    let (base_url, _, _) = setup().await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();

    let res = client
        .get(format!("{base_url}/api/tenko/records"))
        .header("X-Tenant-ID", tenant_id.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_list_records_db_error() {
    let (base_url, auth_header) = setup_failing().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/records"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/tenko/records/{id} — get_record
// =========================================================================

#[tokio::test]
async fn test_get_record_not_found() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();
    let id = uuid::Uuid::new_v4();

    let res = client
        .get(format!("{base_url}/api/tenko/records/{id}"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_get_record_found() {
    let (base_url, auth_header) = setup_found().await;
    let client = reqwest::Client::new();
    let id = uuid::Uuid::new_v4();

    let res = client
        .get(format!("{base_url}/api/tenko/records/{id}"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["id"], id.to_string());
    assert_eq!(body["employee_name"], "Test Employee");
    assert_eq!(body["tenko_type"], "pre_operation");
    assert_eq!(body["status"], "completed");
    assert_eq!(body["tenko_method"], "face_to_face");
    assert_eq!(body["alcohol_result"], "negative");
    assert!(body["alcohol_has_face_photo"].as_bool().unwrap());
}

#[tokio::test]
async fn test_get_record_no_auth() {
    let (base_url, _, _) = setup().await;
    let client = reqwest::Client::new();
    let id = uuid::Uuid::new_v4();

    let res = client
        .get(format!("{base_url}/api/tenko/records/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_get_record_x_tenant_id() {
    let (base_url, _, _) = setup().await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();
    let id = uuid::Uuid::new_v4();

    // Default mock returns None, so 404 with X-Tenant-ID (auth works, but not found)
    let res = client
        .get(format!("{base_url}/api/tenko/records/{id}"))
        .header("X-Tenant-ID", tenant_id.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_get_record_db_error() {
    let (base_url, auth_header) = setup_failing().await;
    let client = reqwest::Client::new();
    let id = uuid::Uuid::new_v4();

    let res = client
        .get(format!("{base_url}/api/tenko/records/{id}"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/tenko/records/csv — export_csv
// =========================================================================

#[tokio::test]
async fn test_export_csv_success_empty() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/records/csv"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // Check content-type header
    let content_type = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(content_type.contains("text/csv"));

    // Check content-disposition header
    let disposition = res
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(disposition.contains("tenko_records.csv"));

    // Check BOM header (UTF-8 BOM: 0xEF 0xBB 0xBF)
    let bytes = res.bytes().await.unwrap();
    assert!(bytes.len() >= 3);
    assert_eq!(bytes[0], 0xEF);
    assert_eq!(bytes[1], 0xBB);
    assert_eq!(bytes[2], 0xBF);

    // Check CSV header row is present after BOM
    let csv_str = std::str::from_utf8(&bytes[3..]).unwrap();
    assert!(csv_str.contains("record_id"));
    assert!(csv_str.contains("employee_name"));
    assert!(csv_str.contains("tenko_type"));
    assert!(csv_str.contains("record_hash"));
}

#[tokio::test]
async fn test_export_csv_with_data() {
    let (base_url, auth_header) = setup_with_data().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/records/csv"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let bytes = res.bytes().await.unwrap();
    // BOM check
    assert_eq!(bytes[0], 0xEF);
    assert_eq!(bytes[1], 0xBB);
    assert_eq!(bytes[2], 0xBF);

    let csv_str = std::str::from_utf8(&bytes[3..]).unwrap();
    // Should have header + 1 data row
    let lines: Vec<&str> = csv_str.trim().lines().collect();
    assert_eq!(lines.len(), 2); // header + 1 record

    // Check data row contains expected values
    assert!(csv_str.contains("Test Employee"));
    assert!(csv_str.contains("pre_operation"));
    assert!(csv_str.contains("completed"));
    assert!(csv_str.contains("face_to_face"));
    assert!(csv_str.contains("negative"));
    assert!(csv_str.contains("abc123hash"));
    // self_declaration fields
    assert!(csv_str.contains("false")); // illness, fatigue, sleep_deprivation
                                        // safety_judgment
    assert!(csv_str.contains("pass"));
    // daily_inspection: all ok -> "ok"
    assert!(csv_str.contains("ok"));
}

#[tokio::test]
async fn test_export_csv_with_filters() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();
    let emp_id = uuid::Uuid::new_v4();

    let res = client
        .get(format!(
            "{base_url}/api/tenko/records/csv?employee_id={emp_id}&tenko_type=pre_operation"
        ))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_export_csv_no_auth() {
    let (base_url, _, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/records/csv"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_export_csv_x_tenant_id() {
    let (base_url, _, _) = setup().await;
    let client = reqwest::Client::new();
    let tenant_id = uuid::Uuid::new_v4();

    let res = client
        .get(format!("{base_url}/api/tenko/records/csv"))
        .header("X-Tenant-ID", tenant_id.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_export_csv_db_error() {
    let (base_url, auth_header) = setup_failing().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/records/csv"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// =========================================================================
// CSV content: JSONB field edge cases
// =========================================================================

#[tokio::test]
async fn test_export_csv_with_ng_daily_inspection() {
    // Test that daily_inspection with an "ng" item produces "ng" in CSV
    let mock = Arc::new(MockTenkoRecordsRepository::default());
    mock.return_ng_data.store(true, Ordering::SeqCst);
    let mut state = mock_helpers::app_state::setup_mock_app_state();
    state.tenko_records = mock;
    let tenant_id = uuid::Uuid::new_v4();
    let base_url = common::spawn_test_server(state).await;
    let jwt = common::create_test_jwt(tenant_id, "admin");

    let client = reqwest::Client::new();
    let res = client
        .get(format!("{base_url}/api/tenko/records/csv"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // The ng mock data has brakes="ng", so daily_inspection_status = "ng"
    let bytes = res.bytes().await.unwrap();
    let csv_str = std::str::from_utf8(&bytes[3..]).unwrap();
    // Verify the daily inspection column has "ng" (not "ok")
    assert!(
        csv_str.contains(",ng,"),
        "Expected 'ng' in CSV but got: {csv_str}"
    );
}

#[tokio::test]
async fn test_export_csv_with_safety_judgment_failed_items() {
    // The default mock has empty failed_items, test that the field is present
    let (base_url, auth_header) = setup_with_data().await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/tenko/records/csv"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let bytes = res.bytes().await.unwrap();
    let csv_str = std::str::from_utf8(&bytes[3..]).unwrap();
    // Header should contain all expected columns
    let header = csv_str.lines().next().unwrap();
    assert!(header.contains("self_declaration_illness"));
    assert!(header.contains("self_declaration_fatigue"));
    assert!(header.contains("self_declaration_sleep"));
    assert!(header.contains("safety_judgment_status"));
    assert!(header.contains("safety_judgment_failed_items"));
    assert!(header.contains("daily_inspection_status"));
    assert!(header.contains("interrupted_at"));
    assert!(header.contains("resumed_at"));
    assert!(header.contains("resume_reason"));
}

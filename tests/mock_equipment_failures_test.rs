mod common;
mod mock_helpers;

use std::sync::Arc;

use chrono::Utc;
use mock_helpers::MockEquipmentFailuresRepository;
use rust_alc_api::db::models::EquipmentFailure;

// ---------------------------------------------------------------------------
// Helper: spawn server with default mock state
// ---------------------------------------------------------------------------

async fn setup() -> (String, String) {
    let state = mock_helpers::app_state::setup_mock_app_state().await;
    let tenant_id = common::create_test_tenant(&state.pool, "ef-test").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let base = common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

/// spawn server with a custom equipment_failures mock
async fn setup_with_mock(mock: Arc<MockEquipmentFailuresRepository>) -> (String, String) {
    let mut state = mock_helpers::app_state::setup_mock_app_state().await;
    let tenant_id = common::create_test_tenant(&state.pool, "ef-custom").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    state.equipment_failures = mock;
    let base = common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

fn make_sample_failure(tenant_id: uuid::Uuid) -> EquipmentFailure {
    EquipmentFailure {
        id: uuid::Uuid::new_v4(),
        tenant_id,
        failure_type: "manual_report".to_string(),
        description: "test failure".to_string(),
        affected_device: Some("device-001".to_string()),
        detected_at: Utc::now(),
        detected_by: Some("operator-a".to_string()),
        resolved_at: Some(Utc::now()),
        resolution_notes: Some("fixed it".to_string()),
        session_id: Some(uuid::Uuid::new_v4()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

/// spawn server with fail_next=true on equipment_failures mock
async fn setup_failing() -> (String, String) {
    let mock = Arc::new(MockEquipmentFailuresRepository::default());
    mock.fail_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = mock_helpers::app_state::setup_mock_app_state().await;
    let tenant_id = common::create_test_tenant(&state.pool, "ef-fail").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    state.equipment_failures = mock;
    let base = common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

// ===========================================================================
// POST /api/tenko/equipment-failures — create_failure
// ===========================================================================

#[tokio::test]
async fn create_failure_success() {
    let (base, auth) = setup().await;
    let res = client()
        .post(format!("{base}/api/tenko/equipment-failures"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "failure_type": "manual_report",
            "description": "test failure"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["failure_type"], "manual_report");
    assert_eq!(body["description"], "test failure");
}

#[tokio::test]
async fn create_failure_invalid_type_returns_400() {
    let (base, auth) = setup().await;
    let res = client()
        .post(format!("{base}/api/tenko/equipment-failures"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "failure_type": "invalid_type",
            "description": "bad type"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn create_failure_db_error_returns_500() {
    let (base, auth) = setup_failing().await;
    let res = client()
        .post(format!("{base}/api/tenko/equipment-failures"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "failure_type": "manual_report",
            "description": "will fail"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

#[tokio::test]
async fn create_failure_no_auth_returns_401() {
    let (base, _auth) = setup().await;
    let res = client()
        .post(format!("{base}/api/tenko/equipment-failures"))
        .json(&serde_json::json!({
            "failure_type": "manual_report",
            "description": "no auth"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

// ===========================================================================
// GET /api/tenko/equipment-failures — list_failures
// ===========================================================================

#[tokio::test]
async fn list_failures_success() {
    let (base, auth) = setup().await;
    let res = client()
        .get(format!("{base}/api/tenko/equipment-failures"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["failures"].is_array());
    assert_eq!(body["total"], 0);
    assert_eq!(body["page"], 1);
    assert_eq!(body["per_page"], 50);
}

#[tokio::test]
async fn list_failures_db_error_returns_500() {
    let (base, auth) = setup_failing().await;
    let res = client()
        .get(format!("{base}/api/tenko/equipment-failures"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// GET /api/tenko/equipment-failures/{id} — get_failure
// ===========================================================================

#[tokio::test]
async fn get_failure_not_found() {
    let (base, auth) = setup().await;
    let id = uuid::Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/tenko/equipment-failures/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    // Mock returns None => 404
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn get_failure_db_error_returns_500() {
    let (base, auth) = setup_failing().await;
    let id = uuid::Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/tenko/equipment-failures/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// PUT /api/tenko/equipment-failures/{id} — resolve_failure
// ===========================================================================

#[tokio::test]
async fn resolve_failure_not_found() {
    let (base, auth) = setup().await;
    let id = uuid::Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/tenko/equipment-failures/{id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "resolution_notes": "fixed"
        }))
        .send()
        .await
        .unwrap();
    // Mock returns None => 404
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn resolve_failure_db_error_returns_500() {
    let (base, auth) = setup_failing().await;
    let id = uuid::Uuid::new_v4();
    let res = client()
        .put(format!("{base}/api/tenko/equipment-failures/{id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "resolution_notes": "will fail"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// GET /api/tenko/equipment-failures/csv — export_csv
// ===========================================================================

#[tokio::test]
async fn export_csv_success() {
    let (base, auth) = setup().await;
    let res = client()
        .get(format!("{base}/api/tenko/equipment-failures/csv"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // Check content-type
    let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.contains("text/csv"));

    // Check content-disposition
    let cd = res
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(cd.contains("equipment_failures.csv"));

    // Check BOM + header row
    let bytes = res.bytes().await.unwrap();
    assert_eq!(
        &bytes[..3],
        &[0xEF, 0xBB, 0xBF],
        "CSV should start with BOM"
    );
    let csv_text = std::str::from_utf8(&bytes[3..]).unwrap();
    assert!(csv_text.contains("id,failure_type,description"));
}

#[tokio::test]
async fn export_csv_db_error_returns_500() {
    let (base, auth) = setup_failing().await;
    let res = client()
        .get(format!("{base}/api/tenko/equipment-failures/csv"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// Validation: all valid failure_type values
// ===========================================================================

#[tokio::test]
async fn create_failure_all_valid_types() {
    let (base, auth) = setup().await;
    let valid_types = [
        "face_recognition_error",
        "measurement_recording_failed",
        "kiosk_offline",
        "database_sync_error",
        "webhook_delivery_failed",
        "session_state_error",
        "photo_storage_error",
        "manual_report",
    ];
    for ft in valid_types {
        let res = client()
            .post(format!("{base}/api/tenko/equipment-failures"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "failure_type": ft,
                "description": format!("test {ft}")
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201, "failure_type '{ft}' should be accepted");
    }
}

// ===========================================================================
// Optional fields in create
// ===========================================================================

#[tokio::test]
async fn create_failure_with_all_optional_fields() {
    let (base, auth) = setup().await;
    let session_id = uuid::Uuid::new_v4();
    let res = client()
        .post(format!("{base}/api/tenko/equipment-failures"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "failure_type": "kiosk_offline",
            "description": "full payload",
            "affected_device": "device-001",
            "detected_at": "2026-03-29T10:00:00Z",
            "detected_by": "operator-a",
            "session_id": session_id.to_string()
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["affected_device"], "device-001");
    assert_eq!(body["detected_by"], "operator-a");
    assert_eq!(body["session_id"], session_id.to_string());
}

// ===========================================================================
// GET /api/tenko/equipment-failures/{id} — get_failure (found)
// ===========================================================================

#[tokio::test]
async fn get_failure_found() {
    let tenant_id = uuid::Uuid::new_v4();
    let failure = make_sample_failure(tenant_id);
    let failure_id = failure.id;
    let mock = Arc::new(MockEquipmentFailuresRepository::default());
    *mock.get_result.lock().unwrap() = Some(failure);
    let (base, auth) = setup_with_mock(mock).await;
    let res = client()
        .get(format!("{base}/api/tenko/equipment-failures/{failure_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["id"], failure_id.to_string());
    assert_eq!(body["failure_type"], "manual_report");
}

// ===========================================================================
// PUT /api/tenko/equipment-failures/{id} — resolve_failure (found)
// ===========================================================================

#[tokio::test]
async fn resolve_failure_found() {
    let tenant_id = uuid::Uuid::new_v4();
    let mut failure = make_sample_failure(tenant_id);
    failure.resolution_notes = Some("resolved via test".to_string());
    let failure_id = failure.id;
    let mock = Arc::new(MockEquipmentFailuresRepository::default());
    *mock.resolve_result.lock().unwrap() = Some(failure);
    let (base, auth) = setup_with_mock(mock).await;
    let res = client()
        .put(format!("{base}/api/tenko/equipment-failures/{failure_id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "resolution_notes": "resolved via test"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["id"], failure_id.to_string());
    assert_eq!(body["resolution_notes"], "resolved via test");
}

// ===========================================================================
// GET /api/tenko/equipment-failures/csv — export_csv with data rows
// ===========================================================================

#[tokio::test]
async fn export_csv_with_data() {
    let tenant_id = uuid::Uuid::new_v4();
    let failure = make_sample_failure(tenant_id);
    let failure_id = failure.id;
    let mock = Arc::new(MockEquipmentFailuresRepository::default());
    *mock.csv_data.lock().unwrap() = vec![failure];
    let (base, auth) = setup_with_mock(mock).await;
    let res = client()
        .get(format!("{base}/api/tenko/equipment-failures/csv"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let bytes = res.bytes().await.unwrap();
    // Skip BOM
    let csv_text = std::str::from_utf8(&bytes[3..]).unwrap();
    // Header row
    assert!(csv_text.contains("id,failure_type,description"));
    // Data row should contain the failure id and fields
    assert!(
        csv_text.contains(&failure_id.to_string()),
        "CSV should contain failure id"
    );
    assert!(csv_text.contains("manual_report"));
    assert!(csv_text.contains("device-001"));
    assert!(csv_text.contains("operator-a"));
    assert!(csv_text.contains("fixed it"));
}

use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::common::mock_storage::MockStorage;
use crate::mock_helpers::app_state::setup_mock_app_state;
use crate::mock_helpers::MockCarinsFilesRepository;
use rust_alc_api::db::repository::carins_files::FileRow;
use rust_alc_api::storage::StorageBackend;

fn test_tenant_id() -> Uuid {
    Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap()
}

fn test_auth_header() -> String {
    let tenant_id = test_tenant_id();
    let jwt = crate::common::create_test_jwt_for_user(
        Uuid::new_v4(),
        tenant_id,
        "mock-test@example.com",
        "admin",
    );
    format!("Bearer {jwt}")
}

fn make_file_row(uuid: &str, s3_key: Option<&str>, blob: Option<&str>) -> FileRow {
    FileRow {
        uuid: uuid.to_string(),
        filename: "test.pdf".to_string(),
        file_type: "application/pdf".to_string(),
        created: "2026-01-01T00:00:00Z".to_string(),
        deleted: None,
        blob: blob.map(|s| s.to_string()),
        s3_key: s3_key.map(|s| s.to_string()),
        storage_class: Some("STANDARD".to_string()),
        last_accessed_at: None,
        access_count_weekly: None,
        access_count_total: None,
        promoted_to_standard_at: None,
    }
}

// =========================================================================
// GET /api/files — success (empty list)
// =========================================================================

#[tokio::test]
async fn test_list_files_success_empty() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/files"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["files"].as_array().unwrap().is_empty());
}

// =========================================================================
// GET /api/files?type=pdf — with type_filter query
// =========================================================================

#[tokio::test]
async fn test_list_files_with_type_filter() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/files?type=application/pdf"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["files"].as_array().unwrap().is_empty());
}

// =========================================================================
// GET /api/files — DB error → 500
// =========================================================================

#[tokio::test]
async fn test_list_files_db_error() {
    let mock_repo = Arc::new(MockCarinsFilesRepository::default());
    mock_repo.fail_next.store(true, Ordering::SeqCst);

    let mut state = setup_mock_app_state();
    state.carins_files = mock_repo;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/files"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// POST /api/files — success (201, base64 decode)
// =========================================================================

#[tokio::test]
async fn test_create_file_success() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    use base64::{engine::general_purpose::STANDARD, Engine};
    let content = STANDARD.encode(b"hello world");

    let res = client
        .post(format!("{base_url}/api/files"))
        .header("Authorization", test_auth_header())
        .json(&serde_json::json!({
            "filename": "test.pdf",
            "type": "application/pdf",
            "content": content
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["filename"], "test.pdf");
    assert_eq!(body["fileType"], "application/pdf");
    assert!(body["s3Key"].as_str().is_some());
}

// =========================================================================
// POST /api/files — invalid base64 → 400
// =========================================================================

#[tokio::test]
async fn test_create_file_invalid_base64() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/files"))
        .header("Authorization", test_auth_header())
        .json(&serde_json::json!({
            "filename": "test.pdf",
            "type": "application/pdf",
            "content": "!!!not-valid-base64!!!"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

// =========================================================================
// POST /api/files — DB error → 500
// =========================================================================

#[tokio::test]
async fn test_create_file_db_error() {
    let mock_repo = Arc::new(MockCarinsFilesRepository::default());
    mock_repo.fail_next.store(true, Ordering::SeqCst);

    let mut state = setup_mock_app_state();
    state.carins_files = mock_repo;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    use base64::{engine::general_purpose::STANDARD, Engine};
    let content = STANDARD.encode(b"hello");

    let res = client
        .post(format!("{base_url}/api/files"))
        .header("Authorization", test_auth_header())
        .json(&serde_json::json!({
            "filename": "test.pdf",
            "type": "application/pdf",
            "content": content
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// POST /api/files — storage upload error → 500
// =========================================================================

#[tokio::test]
async fn test_create_file_storage_error() {
    let mock_storage = Arc::new(MockStorage::new("test-bucket"));
    mock_storage.fail_upload.store(true, Ordering::SeqCst);

    let mut state = setup_mock_app_state();
    state.storage = mock_storage;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    use base64::{engine::general_purpose::STANDARD, Engine};
    let content = STANDARD.encode(b"hello");

    let res = client
        .post(format!("{base_url}/api/files"))
        .header("Authorization", test_auth_header())
        .json(&serde_json::json!({
            "filename": "test.pdf",
            "type": "application/pdf",
            "content": content
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/files/recent — success
// =========================================================================

#[tokio::test]
async fn test_list_recent_success() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/files/recent"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["files"].as_array().unwrap().is_empty());
}

// =========================================================================
// GET /api/files/recent — DB error → 500
// =========================================================================

#[tokio::test]
async fn test_list_recent_db_error() {
    let mock_repo = Arc::new(MockCarinsFilesRepository::default());
    mock_repo.fail_next.store(true, Ordering::SeqCst);

    let mut state = setup_mock_app_state();
    state.carins_files = mock_repo;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/files/recent"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/files/not-attached — success
// =========================================================================

#[tokio::test]
async fn test_list_not_attached_success() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/files/not-attached"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["files"].as_array().unwrap().is_empty());
}

// =========================================================================
// GET /api/files/not-attached — DB error → 500
// =========================================================================

#[tokio::test]
async fn test_list_not_attached_db_error() {
    let mock_repo = Arc::new(MockCarinsFilesRepository::default());
    mock_repo.fail_next.store(true, Ordering::SeqCst);

    let mut state = setup_mock_app_state();
    state.carins_files = mock_repo;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/files/not-attached"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/files/{uuid} — found
// =========================================================================

#[tokio::test]
async fn test_get_file_found() {
    let file_uuid = Uuid::new_v4().to_string();
    let mock_repo = Arc::new(MockCarinsFilesRepository::default());
    *mock_repo.return_file.lock().unwrap() = Some(make_file_row(&file_uuid, None, None));

    let mut state = setup_mock_app_state();
    state.carins_files = mock_repo;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/files/{file_uuid}"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["uuid"], file_uuid);
    assert_eq!(body["filename"], "test.pdf");
}

// =========================================================================
// GET /api/files/{uuid} — not found → 404
// =========================================================================

#[tokio::test]
async fn test_get_file_not_found() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/files/{}", Uuid::new_v4()))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

// =========================================================================
// GET /api/files/{uuid} — DB error → 500
// =========================================================================

#[tokio::test]
async fn test_get_file_db_error() {
    let mock_repo = Arc::new(MockCarinsFilesRepository::default());
    mock_repo.fail_next.store(true, Ordering::SeqCst);

    let mut state = setup_mock_app_state();
    state.carins_files = mock_repo;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/files/{}", Uuid::new_v4()))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/files/{uuid}/download — success (s3_key via carins_storage)
// =========================================================================

#[tokio::test]
async fn test_download_file_s3_key_carins_storage() {
    let s3_key = "tenant123/some-file-key";
    let file_content = b"PDF file content here";

    let mock_repo = Arc::new(MockCarinsFilesRepository::default());
    *mock_repo.return_file.lock().unwrap() = Some(make_file_row("file-uuid-1", Some(s3_key), None));

    let carins_storage = Arc::new(MockStorage::new("carins-bucket"));
    carins_storage
        .upload(s3_key, file_content, "application/pdf")
        .await
        .unwrap();

    let mut state = setup_mock_app_state();
    state.carins_files = mock_repo;
    state.carins_storage = Some(carins_storage);

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/files/file-uuid-1/download"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    assert_eq!(
        res.headers().get("content-type").unwrap().to_str().unwrap(),
        "application/pdf"
    );
    assert!(res
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("test.pdf"));
    let body = res.bytes().await.unwrap();
    assert_eq!(body.as_ref(), file_content);
}

// =========================================================================
// GET /api/files/{uuid}/download — success (s3_key via fallback storage)
// =========================================================================

#[tokio::test]
async fn test_download_file_s3_key_fallback_storage() {
    let s3_key = "tenant123/fallback-key";
    let file_content = b"fallback storage content";

    let mock_repo = Arc::new(MockCarinsFilesRepository::default());
    *mock_repo.return_file.lock().unwrap() = Some(make_file_row("file-uuid-2", Some(s3_key), None));

    // Upload to the main storage (carins_storage is None by default)
    let main_storage = Arc::new(MockStorage::new("test-bucket"));
    main_storage
        .upload(s3_key, file_content, "application/pdf")
        .await
        .unwrap();

    let mut state = setup_mock_app_state();
    state.carins_files = mock_repo;
    state.storage = main_storage;
    // carins_storage remains None, so fallback to state.storage

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/files/file-uuid-2/download"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body = res.bytes().await.unwrap();
    assert_eq!(body.as_ref(), file_content);
}

// =========================================================================
// GET /api/files/{uuid}/download — success (blob, legacy base64)
// =========================================================================

#[tokio::test]
async fn test_download_file_blob() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let original_data = b"blob legacy data";
    let blob_b64 = STANDARD.encode(original_data);

    let mock_repo = Arc::new(MockCarinsFilesRepository::default());
    *mock_repo.return_file.lock().unwrap() =
        Some(make_file_row("file-uuid-3", None, Some(&blob_b64)));

    let mut state = setup_mock_app_state();
    state.carins_files = mock_repo;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/files/file-uuid-3/download"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body = res.bytes().await.unwrap();
    assert_eq!(body.as_ref(), original_data);
}

// =========================================================================
// GET /api/files/{uuid}/download — not found (None from repo) → 404
// =========================================================================

#[tokio::test]
async fn test_download_file_not_found() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/files/{}/download", Uuid::new_v4()))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

// =========================================================================
// GET /api/files/{uuid}/download — neither s3_key nor blob → 404
// =========================================================================

#[tokio::test]
async fn test_download_file_no_s3_key_no_blob() {
    let mock_repo = Arc::new(MockCarinsFilesRepository::default());
    *mock_repo.return_file.lock().unwrap() = Some(make_file_row("file-uuid-empty", None, None));

    let mut state = setup_mock_app_state();
    state.carins_files = mock_repo;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/files/file-uuid-empty/download"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

// =========================================================================
// GET /api/files/{uuid}/download — DB error → 500
// =========================================================================

#[tokio::test]
async fn test_download_file_db_error() {
    let mock_repo = Arc::new(MockCarinsFilesRepository::default());
    mock_repo.fail_next.store(true, Ordering::SeqCst);

    let mut state = setup_mock_app_state();
    state.carins_files = mock_repo;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/files/{}/download", Uuid::new_v4()))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// GET /api/files/{uuid}/download — storage download error → 500
// =========================================================================

#[tokio::test]
async fn test_download_file_storage_error() {
    let s3_key = "tenant123/missing-in-storage";

    let mock_repo = Arc::new(MockCarinsFilesRepository::default());
    *mock_repo.return_file.lock().unwrap() =
        Some(make_file_row("file-uuid-err", Some(s3_key), None));

    // Do NOT upload the file to storage, so download will fail
    let mut state = setup_mock_app_state();
    state.carins_files = mock_repo;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/files/file-uuid-err/download"))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// POST /api/files/{uuid}/delete — success (204)
// =========================================================================

#[tokio::test]
async fn test_delete_file_success() {
    let mock_repo = Arc::new(MockCarinsFilesRepository::default());
    *mock_repo.return_affected.lock().unwrap() = true;

    let mut state = setup_mock_app_state();
    state.carins_files = mock_repo;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/files/{}/delete", Uuid::new_v4()))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 204);
}

// =========================================================================
// POST /api/files/{uuid}/delete — not found → 404
// =========================================================================

#[tokio::test]
async fn test_delete_file_not_found() {
    // default return_affected is false
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/files/{}/delete", Uuid::new_v4()))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

// =========================================================================
// POST /api/files/{uuid}/delete — DB error → 500
// =========================================================================

#[tokio::test]
async fn test_delete_file_db_error() {
    let mock_repo = Arc::new(MockCarinsFilesRepository::default());
    mock_repo.fail_next.store(true, Ordering::SeqCst);

    let mut state = setup_mock_app_state();
    state.carins_files = mock_repo;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/files/{}/delete", Uuid::new_v4()))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// POST /api/files/{uuid}/restore — success (204)
// =========================================================================

#[tokio::test]
async fn test_restore_file_success() {
    let mock_repo = Arc::new(MockCarinsFilesRepository::default());
    *mock_repo.return_affected.lock().unwrap() = true;

    let mut state = setup_mock_app_state();
    state.carins_files = mock_repo;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/files/{}/restore", Uuid::new_v4()))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 204);
}

// =========================================================================
// POST /api/files/{uuid}/restore — not found → 404
// =========================================================================

#[tokio::test]
async fn test_restore_file_not_found() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/files/{}/restore", Uuid::new_v4()))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 404);
}

// =========================================================================
// POST /api/files/{uuid}/restore — DB error → 500
// =========================================================================

#[tokio::test]
async fn test_restore_file_db_error() {
    let mock_repo = Arc::new(MockCarinsFilesRepository::default());
    mock_repo.fail_next.store(true, Ordering::SeqCst);

    let mut state = setup_mock_app_state();
    state.carins_files = mock_repo;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/files/{}/restore", Uuid::new_v4()))
        .header("Authorization", test_auth_header())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// POST /api/files — JSON auto-parse (car inspection JSON)
// =========================================================================

#[tokio::test]
async fn test_create_file_json_auto_parse() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    use base64::{engine::general_purpose::STANDARD, Engine};
    let json_data = serde_json::json!({
        "CertInfo": {
            "ElectCertMgNo": "123456789012",
            "GrantdateE": "令和",
            "GrantdateY": "8",
            "GrantdateM": "2",
            "GrantdateD": "13"
        },
        "CertInfoImportFileVersion": "1.0"
    });
    let content = STANDARD.encode(serde_json::to_vec(&json_data).unwrap());

    let res = client
        .post(format!("{base_url}/api/files"))
        .header("Authorization", test_auth_header())
        .json(&serde_json::json!({
            "filename": "cert.json",
            "type": "application/json",
            "content": content
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 201);
}

// =========================================================================
// POST /api/files — JSON auto-parse with pending PDF match
// =========================================================================

#[tokio::test]
async fn test_create_file_json_auto_parse_with_pending_pdf() {
    let mock_car_inspections =
        Arc::new(crate::mock_helpers::MockCarInspectionRepository::default());
    *mock_car_inspections.pending_pdf_uuid.lock().unwrap() = Some(Uuid::new_v4().to_string());

    let mut state = setup_mock_app_state();
    state.car_inspections = mock_car_inspections;

    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    use base64::{engine::general_purpose::STANDARD, Engine};
    let json_data = serde_json::json!({
        "CertInfo": {
            "ElectCertMgNo": "123456789012",
            "GrantdateE": "令和",
            "GrantdateY": "8",
            "GrantdateM": "2",
            "GrantdateD": "13"
        }
    });
    let content = STANDARD.encode(serde_json::to_vec(&json_data).unwrap());

    let res = client
        .post(format!("{base_url}/api/files"))
        .header("Authorization", test_auth_header())
        .json(&serde_json::json!({
            "filename": "cert.json",
            "type": "application/json",
            "content": content
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 201);
}

// =========================================================================
// POST /api/files — JSON auto-parse error (invalid car inspection JSON)
// =========================================================================

#[tokio::test]
async fn test_create_file_json_parse_error() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    use base64::{engine::general_purpose::STANDARD, Engine};
    let content = STANDARD.encode(b"{}"); // missing CertInfo

    let res = client
        .post(format!("{base_url}/api/files"))
        .header("Authorization", test_auth_header())
        .json(&serde_json::json!({
            "filename": "bad.json",
            "type": "application/json",
            "content": content
        }))
        .send()
        .await
        .unwrap();

    // still 201 — parse error is logged, not returned
    assert_eq!(res.status(), 201);
}

// =========================================================================
// POST /api/files — PDF auto-parse error (non-PDF data)
// =========================================================================

#[tokio::test]
async fn test_create_file_pdf_parse_error() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    use base64::{engine::general_purpose::STANDARD, Engine};
    let content = STANDARD.encode(b"not a pdf file");

    let res = client
        .post(format!("{base_url}/api/files"))
        .header("Authorization", test_auth_header())
        .json(&serde_json::json!({
            "filename": "test.pdf",
            "type": "application/pdf",
            "content": content
        }))
        .send()
        .await
        .unwrap();

    // still 201 — parse error is logged, not returned
    assert_eq!(res.status(), 201);
}

// =========================================================================
// Unauthorized — no JWT → 401
// =========================================================================

#[tokio::test]
async fn test_no_auth_returns_401() {
    let state = setup_mock_app_state();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/files"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    let res = client
        .get(format!("{base_url}/api/files/recent"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    let res = client
        .get(format!("{base_url}/api/files/not-attached"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    let res = client
        .get(format!("{base_url}/api/files/{}", Uuid::new_v4()))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    let res = client
        .get(format!("{base_url}/api/files/{}/download", Uuid::new_v4()))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    let res = client
        .post(format!("{base_url}/api/files/{}/delete", Uuid::new_v4()))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    let res = client
        .post(format!("{base_url}/api/files/{}/restore", Uuid::new_v4()))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    let res = client
        .post(format!("{base_url}/api/files"))
        .json(&serde_json::json!({
            "filename": "test.pdf",
            "type": "application/pdf",
            "content": "aGVsbG8="
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

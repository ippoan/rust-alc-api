use std::sync::Arc;
use uuid::Uuid;

use crate::common::mock_storage::MockStorage;
use crate::mock_helpers::MockTroubleFilesRepository;
use crate::mock_helpers::MockTroubleTicketsRepository;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

async fn setup() -> (String, String) {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

async fn setup_failing_files() -> (String, String) {
    let mock = Arc::new(MockTroubleFilesRepository::default());
    mock.fail_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_files = mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

async fn setup_with_storage() -> (String, String) {
    let tickets_mock = Arc::new(MockTroubleTicketsRepository::default());
    tickets_mock
        .return_some
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_tickets = tickets_mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

// ===========================================================================
// GET /api/trouble/tickets/{ticket_id}/files -- list_files
// ===========================================================================

#[tokio::test]
async fn list_files_success() {
    let (base, auth) = setup().await;
    let ticket_id = Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/trouble/tickets/{ticket_id}/files"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body.is_array());
}

#[tokio::test]
async fn list_files_db_error() {
    let (base, auth) = setup_failing_files().await;
    let ticket_id = Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/trouble/tickets/{ticket_id}/files"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// DELETE /api/trouble/files/{file_id} -- delete_file
// ===========================================================================

#[tokio::test]
async fn delete_file_success() {
    let (base, auth) = setup().await;
    let file_id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/trouble/files/{file_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn delete_file_not_found() {
    let mock = Arc::new(MockTroubleFilesRepository::default());
    mock.delete_returns_false
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_files = mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let file_id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/trouble/files/{file_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn delete_file_db_error() {
    let (base, auth) = setup_failing_files().await;
    let file_id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/trouble/files/{file_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// GET /api/trouble/files/{file_id}/download -- download_file
// ===========================================================================

#[tokio::test]
async fn download_file_not_found() {
    let (base, auth) = setup().await;
    let file_id = Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/trouble/files/{file_id}/download"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    // mock get() returns None => 404
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn download_file_db_error() {
    let (base, auth) = setup_failing_files().await;
    let file_id = Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/trouble/files/{file_id}/download"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// POST /api/trouble/tickets/{ticket_id}/files -- upload_file
// ===========================================================================

#[tokio::test]
async fn upload_file_ticket_not_found() {
    // Default mock: tickets.get() returns None => 404
    let (base, auth) = setup().await;
    let ticket_id = Uuid::new_v4();
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(b"hello".to_vec())
            .file_name("test.txt")
            .mime_str("text/plain")
            .unwrap(),
    );
    let res = client()
        .post(format!("{base}/api/trouble/tickets/{ticket_id}/files"))
        .header("Authorization", &auth)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn upload_file_success() {
    let (base, auth) = setup_with_storage().await;
    let ticket_id = Uuid::new_v4();
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(b"hello".to_vec())
            .file_name("test.txt")
            .mime_str("text/plain")
            .unwrap(),
    );
    let res = client()
        .post(format!("{base}/api/trouble/tickets/{ticket_id}/files"))
        .header("Authorization", &auth)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["filename"], "test.txt");
    assert_eq!(body["content_type"], "text/plain");
    assert_eq!(body["size_bytes"], 5);
}

#[tokio::test]
async fn upload_file_no_storage() {
    // tickets.get() returns Some but trouble_storage is None => 503
    let tickets_mock = Arc::new(MockTroubleTicketsRepository::default());
    tickets_mock
        .return_some
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_tickets = tickets_mock;
    state.trouble_storage = None;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let ticket_id = Uuid::new_v4();
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(b"hello".to_vec())
            .file_name("test.txt")
            .mime_str("text/plain")
            .unwrap(),
    );
    let res = client()
        .post(format!("{base}/api/trouble/tickets/{ticket_id}/files"))
        .header("Authorization", &auth)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 503);
}

#[tokio::test]
async fn download_file_no_storage() {
    // trouble_storage is None, but get() returns None first => 404
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_storage = None;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let file_id = Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/trouble/files/{file_id}/download"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    // mock get() returns None => 404 (before storage check)
    assert_eq!(res.status(), 404);
}

// ===========================================================================
// Download file success (files.get returns Some, storage has data)
// ===========================================================================

#[tokio::test]
async fn download_file_success() {
    let storage = Arc::new(MockStorage::new("trouble-bucket"));
    let storage_key = "tenant/trouble/ticket/file.txt";
    storage.insert_file(storage_key, b"file content".to_vec());

    let files_mock = Arc::new(MockTroubleFilesRepository::default());
    files_mock
        .return_some
        .store(true, std::sync::atomic::Ordering::SeqCst);
    *files_mock.storage_key.lock().unwrap() = storage_key.to_string();

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_files = files_mock;
    state.trouble_storage = Some(storage);
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let file_id = Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/trouble/files/{file_id}/download"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert_eq!(ct, "text/plain");

    let cd = res
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(cd.contains("test.txt"));

    let body = res.bytes().await.unwrap();
    assert_eq!(body.as_ref(), b"file content");
}

// ===========================================================================
// Download file — storage is None but file exists → 503
// ===========================================================================

#[tokio::test]
async fn download_file_no_storage_but_file_exists() {
    let files_mock = Arc::new(MockTroubleFilesRepository::default());
    files_mock
        .return_some
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_files = files_mock;
    state.trouble_storage = None;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let file_id = Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/trouble/files/{file_id}/download"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 503);
}

// ===========================================================================
// Upload file — DB error after storage upload
// ===========================================================================

#[tokio::test]
async fn upload_file_db_error_after_upload() {
    let tickets_mock = Arc::new(MockTroubleTicketsRepository::default());
    tickets_mock
        .return_some
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let files_mock = Arc::new(MockTroubleFilesRepository::default());
    files_mock
        .fail_next
        .store(true, std::sync::atomic::Ordering::SeqCst);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_tickets = tickets_mock;
    state.trouble_files = files_mock;
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let ticket_id = Uuid::new_v4();
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(b"hello".to_vec())
            .file_name("test.txt")
            .mime_str("text/plain")
            .unwrap(),
    );
    let res = client()
        .post(format!("{base}/api/trouble/tickets/{ticket_id}/files"))
        .header("Authorization", &auth)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// Download file — storage download error (key not in storage)
// ===========================================================================

#[tokio::test]
async fn download_file_storage_error() {
    let storage = Arc::new(MockStorage::new("trouble-bucket"));
    // Don't insert any file — download will fail with "Not found"

    let files_mock = Arc::new(MockTroubleFilesRepository::default());
    files_mock
        .return_some
        .store(true, std::sync::atomic::Ordering::SeqCst);
    *files_mock.storage_key.lock().unwrap() = "nonexistent-key".to_string();

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_files = files_mock;
    state.trouble_storage = Some(storage);
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let file_id = Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/trouble/files/{file_id}/download"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// Upload file — storage upload error
// ===========================================================================

#[tokio::test]
async fn upload_file_storage_error() {
    let tickets_mock = Arc::new(MockTroubleTicketsRepository::default());
    tickets_mock
        .return_some
        .store(true, std::sync::atomic::Ordering::SeqCst);

    let storage = Arc::new(MockStorage::new("trouble-bucket"));
    storage
        .fail_upload
        .store(true, std::sync::atomic::Ordering::SeqCst);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    state.trouble_tickets = tickets_mock;
    state.trouble_storage = Some(storage);
    let base = crate::common::spawn_test_server(state).await;
    let auth = format!("Bearer {jwt}");

    let ticket_id = Uuid::new_v4();
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(b"hello".to_vec())
            .file_name("test.txt")
            .mime_str("text/plain")
            .unwrap(),
    );
    let res = client()
        .post(format!("{base}/api/trouble/tickets/{ticket_id}/files"))
        .header("Authorization", &auth)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

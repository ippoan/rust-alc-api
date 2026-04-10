use std::sync::Arc;
use uuid::Uuid;

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

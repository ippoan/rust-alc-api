//! 整備記録の添付ファイル API の DB 不要 (mock) テスト (Refs ippoan/rust-alc-api#651)。
//! `mock_tests/mock_trouble_files_test.rs` が手本。テナント境界と RLS の実測は
//! `tests/maintenance_test.rs` (DB 統合テスト) 側でやる — ここは handler の
//! 分岐 (404 / 503 / 500) だけを DB 無しで固定する。

use std::sync::Arc;
use uuid::Uuid;

use crate::common::mock_storage::MockStorage;
use crate::mock_helpers::MockMaintenanceFilesRepository;

async fn setup() -> (String, String) {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let base = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

// ===========================================================================
// GET /api/maintenance/records/{record_id}/files -- list_files
// ===========================================================================

#[tokio::test]
async fn list_files_success() {
    // デフォルト mock: record_belongs = true
    let (base, auth) = setup().await;
    let record_id = Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/maintenance/records/{record_id}/files"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body.is_array());
}

#[tokio::test]
async fn list_files_record_not_found() {
    let mock = Arc::new(MockMaintenanceFilesRepository::default());
    mock.record_belongs
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let mut maintenance_state = crate::mock_helpers::app_state::setup_mock_maintenance_state();
    maintenance_state.files = mock;
    let base = crate::mock_helpers::app_state::spawn_mock_server_with_maintenance(
        state,
        maintenance_state,
    )
    .await;
    let auth = format!("Bearer {jwt}");

    let record_id = Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/maintenance/records/{record_id}/files"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// ===========================================================================
// POST /api/maintenance/records/{record_id}/files -- upload_file
// ===========================================================================

#[tokio::test]
async fn upload_file_record_not_found() {
    let mock = Arc::new(MockMaintenanceFilesRepository::default());
    mock.record_belongs
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let mut maintenance_state = crate::mock_helpers::app_state::setup_mock_maintenance_state();
    maintenance_state.files = mock;
    let base = crate::mock_helpers::app_state::spawn_mock_server_with_maintenance(
        state,
        maintenance_state,
    )
    .await;
    let auth = format!("Bearer {jwt}");

    let record_id = Uuid::new_v4();
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(b"hello".to_vec())
            .file_name("test.txt")
            .mime_str("text/plain")
            .unwrap(),
    );
    let res = client()
        .post(format!("{base}/api/maintenance/records/{record_id}/files"))
        .header("Authorization", &auth)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn upload_file_success() {
    // デフォルト mock: record_belongs = true, storage は setup_mock_maintenance_state
    // の既定 (Some(MockStorage))
    let (base, auth) = setup_with_maintenance_default().await;
    let record_id = Uuid::new_v4();
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(b"hello".to_vec())
            .file_name("test.txt")
            .mime_str("text/plain")
            .unwrap(),
    );
    let res = client()
        .post(format!("{base}/api/maintenance/records/{record_id}/files"))
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

async fn setup_with_maintenance_default() -> (String, String) {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let maintenance_state = crate::mock_helpers::app_state::setup_mock_maintenance_state();
    let base = crate::mock_helpers::app_state::spawn_mock_server_with_maintenance(
        state,
        maintenance_state,
    )
    .await;
    let auth = format!("Bearer {jwt}");
    (base, auth)
}

#[tokio::test]
async fn upload_file_no_storage() {
    // record_belongs = true (default) だが storage が None => 503
    // (共有 storage が未設定のときの fail-closed。crates/alc-trouble/src/files.rs と同じ)
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let mut maintenance_state = crate::mock_helpers::app_state::setup_mock_maintenance_state();
    maintenance_state.storage = None;
    let base = crate::mock_helpers::app_state::spawn_mock_server_with_maintenance(
        state,
        maintenance_state,
    )
    .await;
    let auth = format!("Bearer {jwt}");

    let record_id = Uuid::new_v4();
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(b"hello".to_vec())
            .file_name("test.txt")
            .mime_str("text/plain")
            .unwrap(),
    );
    let res = client()
        .post(format!("{base}/api/maintenance/records/{record_id}/files"))
        .header("Authorization", &auth)
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 503);
}

// ===========================================================================
// GET /api/maintenance/files/{file_id}/download -- download_file
// ===========================================================================

#[tokio::test]
async fn download_file_not_found() {
    let (base, auth) = setup().await;
    let file_id = Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/maintenance/files/{file_id}/download"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    // mock get() returns None => 404
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn download_file_success() {
    let storage = Arc::new(MockStorage::new("maintenance-bucket"));
    let storage_key = "tenant/maintenance/record/file.txt";
    storage.insert_file(storage_key, b"file content".to_vec());

    let files_mock = Arc::new(MockMaintenanceFilesRepository::default());
    files_mock
        .return_some
        .store(true, std::sync::atomic::Ordering::SeqCst);
    *files_mock.storage_key.lock().unwrap() = storage_key.to_string();

    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let mut maintenance_state = crate::mock_helpers::app_state::setup_mock_maintenance_state();
    maintenance_state.files = files_mock;
    maintenance_state.storage = Some(storage);
    let base = crate::mock_helpers::app_state::spawn_mock_server_with_maintenance(
        state,
        maintenance_state,
    )
    .await;
    let auth = format!("Bearer {jwt}");

    let file_id = Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/maintenance/files/{file_id}/download"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body = res.bytes().await.unwrap();
    assert_eq!(body.as_ref(), b"file content");
}

#[tokio::test]
async fn download_file_no_storage_but_file_exists() {
    let files_mock = Arc::new(MockMaintenanceFilesRepository::default());
    files_mock
        .return_some
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let mut maintenance_state = crate::mock_helpers::app_state::setup_mock_maintenance_state();
    maintenance_state.files = files_mock;
    maintenance_state.storage = None;
    let base = crate::mock_helpers::app_state::spawn_mock_server_with_maintenance(
        state,
        maintenance_state,
    )
    .await;
    let auth = format!("Bearer {jwt}");

    let file_id = Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/maintenance/files/{file_id}/download"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 503);
}

// ===========================================================================
// DELETE /api/maintenance/files/{file_id} -- delete_file
// ===========================================================================

#[tokio::test]
async fn delete_file_success() {
    let (base, auth) = setup().await;
    let file_id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/maintenance/files/{file_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn delete_file_not_found() {
    let mock = Arc::new(MockMaintenanceFilesRepository::default());
    mock.delete_returns_false
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let mut maintenance_state = crate::mock_helpers::app_state::setup_mock_maintenance_state();
    maintenance_state.files = mock;
    let base = crate::mock_helpers::app_state::spawn_mock_server_with_maintenance(
        state,
        maintenance_state,
    )
    .await;
    let auth = format!("Bearer {jwt}");

    let file_id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/maintenance/files/{file_id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

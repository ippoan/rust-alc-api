use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::common::mock_storage::MockStorage;

/// Helper: set up mock AppState and spawn test server with JWT.
/// Returns (base_url, auth_header, tenant_id).
async fn setup() -> (String, String, Uuid) {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header, tenant_id)
}

/// Helper: set up with fail_upload = true on storage.
/// Returns (base_url, auth_header).
async fn setup_failing_storage() -> (String, String) {
    let mock_storage = Arc::new(MockStorage::new("test-bucket"));
    mock_storage.fail_upload.store(true, Ordering::SeqCst);

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.storage = mock_storage;

    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let auth_header = format!("Bearer {jwt}");
    (base_url, auth_header)
}

// =========================================================================
// POST /api/upload/face-photo — success
// =========================================================================

#[tokio::test]
async fn test_upload_face_photo_success() {
    let (base_url, auth_header, tenant_id) = setup().await;
    let client = reqwest::Client::new();

    let part = reqwest::multipart::Part::bytes(vec![0xFF, 0xD8, 0xFF, 0xE0])
        .file_name("test.jpg")
        .mime_str("image/jpeg")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/upload/face-photo"))
        .header("Authorization", &auth_header)
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["url"]
        .as_str()
        .unwrap()
        .contains(&tenant_id.to_string()));
    assert!(body["filename"].as_str().unwrap().ends_with(".jpg"));
}

// =========================================================================
// POST /api/upload/face-photo — storage error → 500
// =========================================================================

#[tokio::test]
async fn test_upload_face_photo_storage_error() {
    let (base_url, auth_header) = setup_failing_storage().await;
    let client = reqwest::Client::new();

    let part = reqwest::multipart::Part::bytes(vec![0xFF, 0xD8])
        .file_name("test.jpg")
        .mime_str("image/jpeg")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/upload/face-photo"))
        .header("Authorization", &auth_header)
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// POST /api/upload/face-photo — no multipart field → 400
// =========================================================================

#[tokio::test]
async fn test_upload_face_photo_no_field() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let form = reqwest::multipart::Form::new();

    let res = client
        .post(format!("{base_url}/api/upload/face-photo"))
        .header("Authorization", &auth_header)
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

// =========================================================================
// POST /api/upload/face-photo — no auth → 401
// =========================================================================

#[tokio::test]
async fn test_upload_face_photo_no_auth() {
    let (base_url, _, _) = setup().await;
    let client = reqwest::Client::new();

    let part = reqwest::multipart::Part::bytes(vec![0xFF])
        .file_name("test.jpg")
        .mime_str("image/jpeg")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/upload/face-photo"))
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

// =========================================================================
// POST /api/upload/report-audio — success
// =========================================================================

#[tokio::test]
async fn test_upload_report_audio_success() {
    let (base_url, auth_header, tenant_id) = setup().await;
    let client = reqwest::Client::new();

    let part = reqwest::multipart::Part::bytes(vec![0x1A, 0x45, 0xDF, 0xA3])
        .file_name("report.webm")
        .mime_str("audio/webm")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/upload/report-audio"))
        .header("Authorization", &auth_header)
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    let url = body["url"].as_str().unwrap();
    assert!(url.contains(&tenant_id.to_string()));
    assert!(url.contains("report-audio"));
    assert!(body["filename"].as_str().unwrap().ends_with(".webm"));
}

// =========================================================================
// POST /api/upload/report-audio — storage error → 500
// =========================================================================

#[tokio::test]
async fn test_upload_report_audio_storage_error() {
    let (base_url, auth_header) = setup_failing_storage().await;
    let client = reqwest::Client::new();

    let part = reqwest::multipart::Part::bytes(vec![0x1A])
        .file_name("report.webm")
        .mime_str("audio/webm")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/upload/report-audio"))
        .header("Authorization", &auth_header)
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// POST /api/upload/report-audio — no multipart field → 400
// =========================================================================

#[tokio::test]
async fn test_upload_report_audio_no_field() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let form = reqwest::multipart::Form::new();

    let res = client
        .post(format!("{base_url}/api/upload/report-audio"))
        .header("Authorization", &auth_header)
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

// =========================================================================
// POST /api/upload/report-audio — no auth → 401
// =========================================================================

#[tokio::test]
async fn test_upload_report_audio_no_auth() {
    let (base_url, _, _) = setup().await;
    let client = reqwest::Client::new();

    let part = reqwest::multipart::Part::bytes(vec![0x1A])
        .file_name("report.webm")
        .mime_str("audio/webm")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/upload/report-audio"))
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

// =========================================================================
// POST /api/upload/blow-video — success
// =========================================================================

#[tokio::test]
async fn test_upload_blow_video_success() {
    let (base_url, auth_header, tenant_id) = setup().await;
    let client = reqwest::Client::new();

    let part = reqwest::multipart::Part::bytes(vec![0x1A, 0x45, 0xDF, 0xA3])
        .file_name("blow.webm")
        .mime_str("video/webm")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/upload/blow-video"))
        .header("Authorization", &auth_header)
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    let url = body["url"].as_str().unwrap();
    assert!(url.contains(&tenant_id.to_string()));
    assert!(url.contains("blow-video"));
    assert!(body["filename"].as_str().unwrap().ends_with(".webm"));
}

// =========================================================================
// POST /api/upload/blow-video — storage error → 500
// =========================================================================

#[tokio::test]
async fn test_upload_blow_video_storage_error() {
    let (base_url, auth_header) = setup_failing_storage().await;
    let client = reqwest::Client::new();

    let part = reqwest::multipart::Part::bytes(vec![0x1A])
        .file_name("blow.webm")
        .mime_str("video/webm")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/upload/blow-video"))
        .header("Authorization", &auth_header)
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 500);
}

// =========================================================================
// POST /api/upload/blow-video — no multipart field → 400
// =========================================================================

#[tokio::test]
async fn test_upload_blow_video_no_field() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let form = reqwest::multipart::Form::new();

    let res = client
        .post(format!("{base_url}/api/upload/blow-video"))
        .header("Authorization", &auth_header)
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

// =========================================================================
// POST /api/upload/blow-video — no auth → 401
// =========================================================================

#[tokio::test]
async fn test_upload_blow_video_no_auth() {
    let (base_url, _, _) = setup().await;
    let client = reqwest::Client::new();

    let part = reqwest::multipart::Part::bytes(vec![0x1A])
        .file_name("blow.webm")
        .mime_str("video/webm")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/upload/blow-video"))
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401);
}

// =========================================================================
// POST /api/upload/face-photo — invalid multipart body → 400
// (triggers next_field() map_err path)
// =========================================================================

#[tokio::test]
async fn test_upload_face_photo_invalid_multipart() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    // Send raw body with multipart content-type but invalid body
    let res = client
        .post(format!("{base_url}/api/upload/face-photo"))
        .header("Authorization", &auth_header)
        .header("Content-Type", "multipart/form-data; boundary=INVALID")
        .body("not a valid multipart body")
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

// =========================================================================
// POST /api/upload/report-audio — invalid multipart body → 400
// =========================================================================

#[tokio::test]
async fn test_upload_report_audio_invalid_multipart() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/upload/report-audio"))
        .header("Authorization", &auth_header)
        .header("Content-Type", "multipart/form-data; boundary=INVALID")
        .body("not a valid multipart body")
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

// =========================================================================
// POST /api/upload/blow-video — invalid multipart body → 400
// =========================================================================

#[tokio::test]
async fn test_upload_blow_video_invalid_multipart() {
    let (base_url, auth_header, _) = setup().await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/upload/blow-video"))
        .header("Authorization", &auth_header)
        .header("Content-Type", "multipart/form-data; boundary=INVALID")
        .body("not a valid multipart body")
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 400);
}

// =========================================================================
// X-Tenant-ID header fallback (no JWT) — success
// =========================================================================

#[tokio::test]
async fn test_upload_face_photo_with_tenant_header() {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    let part = reqwest::multipart::Part::bytes(vec![0xFF, 0xD8, 0xFF, 0xE0])
        .file_name("test.jpg")
        .mime_str("image/jpeg")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let res = client
        .post(format!("{base_url}/api/upload/face-photo"))
        .header("X-Tenant-ID", tenant_id.to_string())
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["url"]
        .as_str()
        .unwrap()
        .contains(&tenant_id.to_string()));
    assert!(body["filename"].as_str().unwrap().ends_with(".jpg"));
}

mod common;

use serde_json::Value;

// ============================================================
// carins_files: GET エンドポイント
// ============================================================

#[tokio::test]
async fn test_list_files() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "CarinsFiles").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/files"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["files"].is_array());
}

#[tokio::test]
async fn test_list_files_with_type_filter() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "CarinsType").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/files?type=image/jpeg"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_list_recent_files() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "CarinsRecent").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/files/recent"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_list_not_attached_files() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "CarinsNotAtt").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/files/not-attached"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_files_requires_auth() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    for endpoint in ["/api/files", "/api/files/recent", "/api/files/not-attached"] {
        let res = client
            .get(format!("{base_url}{endpoint}"))
            .send().await.unwrap();
        assert_eq!(res.status(), 401, "Expected 401 for {endpoint}");
    }
}

#[tokio::test]
async fn test_get_file_not_found() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "CarinsGetNF").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let fake_uuid = uuid::Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/files/{fake_uuid}"))
        .header("Authorization", format!("Bearer {jwt}"))
        .send().await.unwrap();
    assert_eq!(res.status(), 404);
}

// ============================================================
// upload: multipart upload
// ============================================================

#[tokio::test]
async fn test_upload_face_photo() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "UploadFace").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let file_part = reqwest::multipart::Part::bytes(b"fake-jpeg-data".to_vec())
        .file_name("test.jpg")
        .mime_str("image/jpeg")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", file_part);

    let res = client
        .post(format!("{base_url}/api/upload/face-photo"))
        .header("Authorization", format!("Bearer {jwt}"))
        .multipart(form)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["url"].as_str().is_some());
}

#[tokio::test]
async fn test_upload_report_audio() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "UploadAudio").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let file_part = reqwest::multipart::Part::bytes(b"fake-audio".to_vec())
        .file_name("test.webm")
        .mime_str("audio/webm")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", file_part);

    let res = client
        .post(format!("{base_url}/api/upload/report-audio"))
        .header("Authorization", format!("Bearer {jwt}"))
        .multipart(form)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["url"].as_str().is_some());
}

#[tokio::test]
async fn test_upload_blow_video() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(&state.pool, "UploadVideo").await;
    let jwt = common::create_test_jwt(tenant_id, "admin");
    let client = reqwest::Client::new();

    let file_part = reqwest::multipart::Part::bytes(b"fake-video".to_vec())
        .file_name("test.webm")
        .mime_str("video/webm")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", file_part);

    let res = client
        .post(format!("{base_url}/api/upload/blow-video"))
        .header("Authorization", format!("Bearer {jwt}"))
        .multipart(form)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert!(body["url"].as_str().is_some());
}

#[tokio::test]
async fn test_upload_requires_auth() {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    for endpoint in ["/api/upload/face-photo", "/api/upload/report-audio", "/api/upload/blow-video"] {
        let res = client
            .post(format!("{base_url}{endpoint}"))
            .send().await.unwrap();
        assert_eq!(res.status(), 401, "Expected 401 for {endpoint}");
    }
}

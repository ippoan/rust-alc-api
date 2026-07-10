//! camera route が monolith で応答することの検証 (Refs #556)。
//!
//! per-domain の alc-camera-api を廃止し monolith へ移植したため、`/api/cameras*`
//! が monolith router 上で tenant スコープ配下に応答することを mock で確かめる。

use std::sync::Arc;

use uuid::Uuid;

use crate::mock_helpers::app_state::MockCamerasRepository;

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

/// デフォルト mock camera state でサーバーを起動する。
async fn setup() -> (String, String) {
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let base = crate::mock_helpers::app_state::spawn_mock_server(state).await;
    (base, format!("Bearer {jwt}"))
}

/// fail_next=true の camera mock でサーバーを起動する (DB エラー注入)。
async fn setup_failing() -> (String, String) {
    let mock = Arc::new(MockCamerasRepository::default());
    mock.fail_next
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let state = crate::mock_helpers::app_state::setup_mock_app_state();
    let mut camera_state = crate::mock_helpers::app_state::setup_mock_camera_state();
    camera_state.cameras = mock;
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");
    let base =
        crate::mock_helpers::app_state::spawn_mock_server_with_camera(state, camera_state).await;
    (base, format!("Bearer {jwt}"))
}

// ===========================================================================
// GET /api/cameras — list_cameras
// ===========================================================================

#[tokio::test]
async fn list_cameras_success() {
    let (base, auth) = setup().await;
    let res = client()
        .get(format!("{base}/api/cameras"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body.is_array());
}

#[tokio::test]
async fn list_cameras_no_auth_returns_401() {
    let (base, _auth) = setup().await;
    let res = client()
        .get(format!("{base}/api/cameras"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn list_cameras_db_error_returns_500() {
    let (base, auth) = setup_failing().await;
    let res = client()
        .get(format!("{base}/api/cameras"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ===========================================================================
// POST /api/cameras — create_camera
// ===========================================================================

#[tokio::test]
async fn create_camera_success() {
    let (base, auth) = setup().await;
    let res = client()
        .post(format!("{base}/api/cameras"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "name": "entrance",
            "ip": "192.168.0.20",
            "onvif_port": 2020,
            "model": "Tapo"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["id"].is_string());
}

// ===========================================================================
// GET /api/cameras/status — camera_statuses
// ===========================================================================

#[tokio::test]
async fn camera_statuses_success() {
    let (base, auth) = setup().await;
    let res = client()
        .get(format!("{base}/api/cameras/status"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body.is_array());
}

// ===========================================================================
// GET/PATCH/DELETE /api/cameras/{id}
// ===========================================================================

#[tokio::test]
async fn get_camera_success() {
    let (base, auth) = setup().await;
    let id = Uuid::new_v4();
    let res = client()
        .get(format!("{base}/api/cameras/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    // mock は Some を返すので 200。
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn update_camera_success() {
    let (base, auth) = setup().await;
    let id = Uuid::new_v4();
    let res = client()
        .patch(format!("{base}/api/cameras/{id}"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({ "active": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn delete_camera_success() {
    let (base, auth) = setup().await;
    let id = Uuid::new_v4();
    let res = client()
        .delete(format!("{base}/api/cameras/{id}"))
        .header("Authorization", &auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

// ===========================================================================
// POST /api/cameras/{id}/health-logs — create_health_log
// ===========================================================================

#[tokio::test]
async fn create_health_log_success() {
    let (base, auth) = setup().await;
    let id = Uuid::new_v4();
    let res = client()
        .post(format!("{base}/api/cameras/{id}/health-logs"))
        .header("Authorization", &auth)
        .json(&serde_json::json!({
            "alive": true,
            "latency_ms": 42
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);
}

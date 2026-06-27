//! Internal viewer endpoints (`/api/internal/notify/view/*`) のモックテスト。
//!
//! lockdown 後の viewer Worker (OIDC `aud=alc-api-internal`) 用に、r2_key + メタを
//! 返す internal view と既読化 internal read を `require_internal_jwt` 配下で提供する
//! 経路 (Refs #434)。実 DB は使わず MockNotifyDeliveryRepository で振る舞いを固定する。

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde_json::Value;
use uuid::Uuid;

use rust_alc_api::db::repository::notify_deliveries::DeliveryViewInfo;

use crate::mock_helpers::app_state::setup_mock_app_state;
use crate::mock_helpers::MockNotifyDeliveryRepository;

fn install_mock(state: &mut rust_alc_api::AppState) -> Arc<MockNotifyDeliveryRepository> {
    let mock = Arc::new(MockNotifyDeliveryRepository::default());
    state.notify_deliveries = mock.clone();
    mock
}

fn sample_view(expire_in_hours: i64) -> DeliveryViewInfo {
    DeliveryViewInfo {
        document_id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        r2_key: "tenant/email/msg/file.pdf".into(),
        file_name: Some("file.pdf".into()),
        file_size_bytes: Some(2048),
        source_subject: Some("件名".into()),
        source_sender: Some("from@example.com".into()),
        source_received_at: Some(chrono::Utc::now()),
        expire_at: chrono::Utc::now() + chrono::Duration::hours(expire_in_hours),
    }
}

// ============================================================
// GET /api/internal/notify/view/{token}
// ============================================================

#[tokio::test]
async fn test_internal_view_success() {
    test_group!("Internal: notify view success");
    test_case!("有効な token は r2_key + メタを返す", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
        let mut state = setup_mock_app_state();
        let _mock = install_mock(&mut state);
        let base_url = crate::common::spawn_test_server(state).await;

        let token = Uuid::new_v4();
        let jwt = crate::common::create_test_internal_jwt();
        let res = reqwest::Client::new()
            .get(format!("{base_url}/api/internal/notify/view/{token}"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        // internal は公開 metadata と違い r2_key を含む
        assert_eq!(body["r2_key"], "mock/key");
        assert_eq!(body["file_name"], "mock.pdf");
        assert_eq!(body["file_size_bytes"], 1024);
        // document_id / tenant_id は露出しない
        assert!(body.get("document_id").is_none());
        assert!(body.get("tenant_id").is_none());
    });
}

#[tokio::test]
async fn test_internal_view_not_found() {
    test_group!("Internal: notify view not found");
    test_case!("存在しない token は 404", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
        let mut state = setup_mock_app_state();
        let mock = install_mock(&mut state);
        *mock.view_override.lock().unwrap() = Some(None);
        let base_url = crate::common::spawn_test_server(state).await;

        let token = Uuid::new_v4();
        let jwt = crate::common::create_test_internal_jwt();
        let res = reqwest::Client::new()
            .get(format!("{base_url}/api/internal/notify/view/{token}"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

#[tokio::test]
async fn test_internal_view_expired() {
    test_group!("Internal: notify view expired");
    test_case!("期限切れ token は 410 Gone", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
        let mut state = setup_mock_app_state();
        let mock = install_mock(&mut state);
        *mock.view_override.lock().unwrap() = Some(Some(sample_view(-1)));
        let base_url = crate::common::spawn_test_server(state).await;

        let token = Uuid::new_v4();
        let jwt = crate::common::create_test_internal_jwt();
        let res = reqwest::Client::new()
            .get(format!("{base_url}/api/internal/notify/view/{token}"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 410);
    });
}

#[tokio::test]
async fn test_internal_view_db_error() {
    test_group!("Internal: notify view db error");
    test_case!("get_for_view が失敗すると 500", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
        let mut state = setup_mock_app_state();
        let mock = install_mock(&mut state);
        mock.fail_next.store(true, Ordering::SeqCst);
        let base_url = crate::common::spawn_test_server(state).await;

        let token = Uuid::new_v4();
        let jwt = crate::common::create_test_internal_jwt();
        let res = reqwest::Client::new()
            .get(format!("{base_url}/api/internal/notify/view/{token}"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

#[tokio::test]
async fn test_internal_view_requires_internal_jwt() {
    test_group!("Internal: notify view auth required");
    test_case!("internal JWT なしは 401", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
        let mut state = setup_mock_app_state();
        let _mock = install_mock(&mut state);
        let base_url = crate::common::spawn_test_server(state).await;

        let token = Uuid::new_v4();
        let res = reqwest::Client::new()
            .get(format!("{base_url}/api/internal/notify/view/{token}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 401);
    });
}

// ============================================================
// POST /api/internal/notify/view/{token}/read
// ============================================================

#[tokio::test]
async fn test_internal_mark_read_success() {
    test_group!("Internal: notify mark read success");
    test_case!("有効な token の既読化は 204", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
        let mut state = setup_mock_app_state();
        let _mock = install_mock(&mut state);
        let base_url = crate::common::spawn_test_server(state).await;

        let token = Uuid::new_v4();
        let jwt = crate::common::create_test_internal_jwt();
        let res = reqwest::Client::new()
            .post(format!("{base_url}/api/internal/notify/view/{token}/read"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 204);
    });
}

#[tokio::test]
async fn test_internal_mark_read_not_found() {
    test_group!("Internal: notify mark read not found");
    test_case!("存在しない token の既読化は 404", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
        let mut state = setup_mock_app_state();
        let mock = install_mock(&mut state);
        mock.mark_read_none.store(true, Ordering::SeqCst);
        let base_url = crate::common::spawn_test_server(state).await;

        let token = Uuid::new_v4();
        let jwt = crate::common::create_test_internal_jwt();
        let res = reqwest::Client::new()
            .post(format!("{base_url}/api/internal/notify/view/{token}/read"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

#[tokio::test]
async fn test_internal_mark_read_db_error() {
    test_group!("Internal: notify mark read db error");
    test_case!("mark_read が失敗すると 500", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
        let mut state = setup_mock_app_state();
        let mock = install_mock(&mut state);
        mock.fail_next.store(true, Ordering::SeqCst);
        let base_url = crate::common::spawn_test_server(state).await;

        let token = Uuid::new_v4();
        let jwt = crate::common::create_test_internal_jwt();
        let res = reqwest::Client::new()
            .post(format!("{base_url}/api/internal/notify/view/{token}/read"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

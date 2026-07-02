use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde_json::Value;
use uuid::Uuid;

use rust_alc_api::db::repository::lineworks_channels::BotConfigForWebhook;

use crate::mock_helpers::app_state::setup_mock_app_state;
use crate::mock_helpers::MockLineworksChannelsRepository;

fn install_mock(
    state: &mut rust_alc_api::AppState,
    cfg: Option<BotConfigForWebhook>,
) -> Arc<MockLineworksChannelsRepository> {
    let mock = Arc::new(MockLineworksChannelsRepository::default());
    *mock.bot_config.lock().unwrap() = cfg;
    state.lineworks_channels = mock.clone();
    mock
}

fn sample_bot_cfg() -> BotConfigForWebhook {
    BotConfigForWebhook {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        bot_secret_encrypted: Some("aGVsbG8=".to_string()),
    }
}

// ============================================================
// GET /api/internal/lineworks/bot-secret/{bot_id}
// ============================================================

#[tokio::test]
async fn test_get_bot_secret_success() {
    test_group!("Internal: get_bot_secret success");
    test_case!("登録済み bot は暗号化済み bot_secret を返す", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        let mut state = setup_mock_app_state();
        let _mock = install_mock(&mut state, Some(sample_bot_cfg()));
        let base_url = crate::common::spawn_test_server(state).await;

        let jwt = crate::common::create_test_internal_jwt();
        let res = reqwest::Client::new()
            .get(format!(
                "{base_url}/api/internal/lineworks/bot-secret/test-bot"
            ))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["bot_secret_encrypted"], "aGVsbG8=");
    });
}

#[tokio::test]
async fn test_get_bot_secret_not_found() {
    test_group!("Internal: get_bot_secret not found");
    test_case!("未登録 bot_id は 404", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        let mut state = setup_mock_app_state();
        let _mock = install_mock(&mut state, None);
        let base_url = crate::common::spawn_test_server(state).await;

        let jwt = crate::common::create_test_internal_jwt();
        let res = reqwest::Client::new()
            .get(format!("{base_url}/api/internal/lineworks/bot-secret/none"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

#[tokio::test]
async fn test_get_bot_secret_no_secret_configured() {
    test_group!("Internal: get_bot_secret no secret");
    test_case!(
        "bot 自体は存在するが bot_secret 未設定なら 404",
        {
            let _guard = crate::common::ENV_LOCK.lock().unwrap();
            std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
            let mut state = setup_mock_app_state();
            let mut cfg = sample_bot_cfg();
            cfg.bot_secret_encrypted = None;
            let _mock = install_mock(&mut state, Some(cfg));
            let base_url = crate::common::spawn_test_server(state).await;

            let jwt = crate::common::create_test_internal_jwt();
            let res = reqwest::Client::new()
                .get(format!("{base_url}/api/internal/lineworks/bot-secret/x"))
                .header("Authorization", format!("Bearer {jwt}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 404);
        }
    );
}

#[tokio::test]
async fn test_get_bot_secret_unauthorized_without_jwt() {
    test_group!("Internal: get_bot_secret no auth");
    test_case!("Authorization ヘッダー無しは 401", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        let mut state = setup_mock_app_state();
        let _mock = install_mock(&mut state, Some(sample_bot_cfg()));
        let base_url = crate::common::spawn_test_server(state).await;

        let res = reqwest::Client::new()
            .get(format!("{base_url}/api/internal/lineworks/bot-secret/x"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 401);
    });
}

#[tokio::test]
async fn test_get_bot_secret_user_jwt_rejected() {
    test_group!("Internal: get_bot_secret user JWT");
    test_case!("ユーザー JWT (aud 無し) は 401", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        let mut state = setup_mock_app_state();
        let _mock = install_mock(&mut state, Some(sample_bot_cfg()));
        let base_url = crate::common::spawn_test_server(state).await;

        let user_jwt = crate::common::create_test_jwt(Uuid::new_v4(), "admin");
        let res = reqwest::Client::new()
            .get(format!("{base_url}/api/internal/lineworks/bot-secret/x"))
            .header("Authorization", format!("Bearer {user_jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 401);
    });
}

#[tokio::test]
async fn test_get_bot_secret_db_error() {
    test_group!("Internal: get_bot_secret DB error");
    test_case!("lookup 失敗時は 500", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        let mut state = setup_mock_app_state();
        let mock = install_mock(&mut state, Some(sample_bot_cfg()));
        mock.fail_next.store(true, Ordering::SeqCst);
        let base_url = crate::common::spawn_test_server(state).await;

        let jwt = crate::common::create_test_internal_jwt();
        let res = reqwest::Client::new()
            .get(format!("{base_url}/api/internal/lineworks/bot-secret/x"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

// ============================================================
// POST /api/internal/lineworks/event
// ============================================================

async fn post_event(base_url: &str, jwt: &str, body: Value) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{base_url}/api/internal/lineworks/event"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&body)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn test_event_joined_calls_upsert() {
    test_group!("Internal: event joined");
    test_case!("joined イベントで upsert_joined が呼ばれる", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        let mut state = setup_mock_app_state();
        let mock = install_mock(&mut state, Some(sample_bot_cfg()));
        let base_url = crate::common::spawn_test_server(state).await;

        let jwt = crate::common::create_test_internal_jwt();
        let res = post_event(
            &base_url,
            &jwt,
            serde_json::json!({
                "bot_id": "bot-1",
                "event_type": "joined",
                "channel_id": "ch-1",
                "channel_type": "group",
                "title": "テスト"
            }),
        )
        .await;
        assert_eq!(res.status(), 200);
        assert_eq!(mock.upsert_joined_calls.load(Ordering::SeqCst), 1);
        assert_eq!(mock.mark_left_calls.load(Ordering::SeqCst), 0);
    });
}

#[tokio::test]
async fn test_event_left_calls_mark_left() {
    test_group!("Internal: event left");
    test_case!("left イベントで mark_left が呼ばれる", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        let mut state = setup_mock_app_state();
        let mock = install_mock(&mut state, Some(sample_bot_cfg()));
        let base_url = crate::common::spawn_test_server(state).await;

        let jwt = crate::common::create_test_internal_jwt();
        let res = post_event(
            &base_url,
            &jwt,
            serde_json::json!({
                "bot_id": "bot-1",
                "event_type": "left",
                "channel_id": "ch-1"
            }),
        )
        .await;
        assert_eq!(res.status(), 200);
        assert_eq!(mock.mark_left_calls.load(Ordering::SeqCst), 1);
        assert_eq!(mock.upsert_joined_calls.load(Ordering::SeqCst), 0);
    });
}

#[tokio::test]
async fn test_event_unknown_type_ignored() {
    test_group!("Internal: event unknown type");
    test_case!("未知の event_type は無視されて 200", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        let mut state = setup_mock_app_state();
        let mock = install_mock(&mut state, Some(sample_bot_cfg()));
        let base_url = crate::common::spawn_test_server(state).await;

        let jwt = crate::common::create_test_internal_jwt();
        let res = post_event(
            &base_url,
            &jwt,
            serde_json::json!({
                "bot_id": "bot-1",
                "event_type": "message",
                "channel_id": "ch-1"
            }),
        )
        .await;
        assert_eq!(res.status(), 200);
        assert_eq!(mock.upsert_joined_calls.load(Ordering::SeqCst), 0);
        assert_eq!(mock.mark_left_calls.load(Ordering::SeqCst), 0);
    });
}

#[tokio::test]
async fn test_event_no_channel_id_skipped() {
    test_group!("Internal: event no channel_id");
    test_case!(
        "channel_id 無しは upsert/mark_left 共に呼ばずに 200",
        {
            let _guard = crate::common::ENV_LOCK.lock().unwrap();
            std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
            let mut state = setup_mock_app_state();
            let mock = install_mock(&mut state, Some(sample_bot_cfg()));
            let base_url = crate::common::spawn_test_server(state).await;

            let jwt = crate::common::create_test_internal_jwt();
            let res = post_event(
                &base_url,
                &jwt,
                serde_json::json!({
                    "bot_id": "bot-1",
                    "event_type": "joined"
                }),
            )
            .await;
            assert_eq!(res.status(), 200);
            assert_eq!(mock.upsert_joined_calls.load(Ordering::SeqCst), 0);
        }
    );
}

#[tokio::test]
async fn test_event_bot_not_found() {
    test_group!("Internal: event bot not found");
    test_case!("未登録 bot_id は 404", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        let mut state = setup_mock_app_state();
        let _mock = install_mock(&mut state, None);
        let base_url = crate::common::spawn_test_server(state).await;

        let jwt = crate::common::create_test_internal_jwt();
        let res = post_event(
            &base_url,
            &jwt,
            serde_json::json!({
                "bot_id": "missing",
                "event_type": "joined",
                "channel_id": "ch-1"
            }),
        )
        .await;
        assert_eq!(res.status(), 404);
    });
}

#[tokio::test]
async fn test_event_unauthorized() {
    test_group!("Internal: event unauthorized");
    test_case!("JWT 無しは 401", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        let mut state = setup_mock_app_state();
        let _mock = install_mock(&mut state, Some(sample_bot_cfg()));
        let base_url = crate::common::spawn_test_server(state).await;

        let res = reqwest::Client::new()
            .post(format!("{base_url}/api/internal/lineworks/event"))
            .json(&serde_json::json!({
                "bot_id": "bot-1",
                "event_type": "joined",
                "channel_id": "ch-1"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 401);
    });
}

#[tokio::test]
async fn test_event_lookup_db_error() {
    test_group!("Internal: event DB error");
    test_case!("lookup 失敗時は 500", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        let mut state = setup_mock_app_state();
        let mock = install_mock(&mut state, Some(sample_bot_cfg()));
        mock.fail_next.store(true, Ordering::SeqCst);
        let base_url = crate::common::spawn_test_server(state).await;

        let jwt = crate::common::create_test_internal_jwt();
        let res = post_event(
            &base_url,
            &jwt,
            serde_json::json!({
                "bot_id": "bot-1",
                "event_type": "joined",
                "channel_id": "ch-1"
            }),
        )
        .await;
        assert_eq!(res.status(), 500);
    });
}

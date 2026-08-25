use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde_json::Value;
use uuid::Uuid;

use rust_alc_api::db::repository::lineworks_channels::BotConfigForWebhook;

use crate::mock_helpers::app_state::setup_mock_app_state;
use crate::mock_helpers::{MockLineworksChannelsRepository, MockNotifyRecipientRepository};

fn install_mock(
    state: &mut rust_alc_api::AppState,
    cfg: Option<BotConfigForWebhook>,
) -> Arc<MockLineworksChannelsRepository> {
    let mock = Arc::new(MockLineworksChannelsRepository::default());
    *mock.bot_config.lock().unwrap() = cfg;
    state.lineworks_channels = mock.clone();
    mock
}

/// recipient 宛 (`recipient_id`) 経路用の mock を差し込む。
fn install_recipient_mock(
    state: &mut rust_alc_api::AppState,
) -> Arc<MockNotifyRecipientRepository> {
    let mock = Arc::new(MockNotifyRecipientRepository::default());
    state.notify_recipients = mock.clone();
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
        let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

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
        let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

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
            let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

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
        let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

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
        let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

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
        let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

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
        let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

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
        let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

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
        let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

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
            let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

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
        let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

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
        let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

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
        let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

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

// ============================================================
// POST /api/internal/lineworks/send
// ============================================================

async fn post_send(base_url: &str, jwt: &str, body: Value) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{base_url}/api/internal/lineworks/send"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&body)
        .send()
        .await
        .unwrap()
}

fn send_body(text: &str) -> Value {
    serde_json::json!({ "channel_id": Uuid::new_v4(), "text": text })
}

#[tokio::test]
async fn test_send_reaches_bot_config_lookup() {
    test_group!("Internal: send reaches bot config");
    test_case!(
        "channel は id 引きで tenant ごと解決され、bot_config 不在なら 500",
        {
            let _guard = crate::common::ENV_LOCK.lock().unwrap();
            std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
            let mut state = setup_mock_app_state();
            let _mock = install_mock(&mut state, Some(sample_bot_cfg()));
            let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

            let jwt = crate::common::create_test_internal_jwt();
            let res = post_send(&base_url, &jwt, send_body("予約番号は J5JZPEQJ です")).await;
            assert_eq!(res.status(), 500);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["message"], "bot_config_not_found");
        }
    );
}

#[tokio::test]
async fn test_send_channel_not_found() {
    test_group!("Internal: send channel not found");
    test_case!("未登録 channel id は 404", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        let mut state = setup_mock_app_state();
        let mock = install_mock(&mut state, Some(sample_bot_cfg()));
        *mock.send_channel.lock().unwrap() = None;
        let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

        let jwt = crate::common::create_test_internal_jwt();
        let res = post_send(&base_url, &jwt, send_body("hello")).await;
        assert_eq!(res.status(), 404);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["error"], "channel_not_found");
    });
}

#[tokio::test]
async fn test_send_empty_text_rejected() {
    test_group!("Internal: send empty text");
    test_case!(
        "空文字 / 空白のみの text は 400 (channel も引かない)",
        {
            let _guard = crate::common::ENV_LOCK.lock().unwrap();
            std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
            let mut state = setup_mock_app_state();
            let _mock = install_mock(&mut state, Some(sample_bot_cfg()));
            let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

            let jwt = crate::common::create_test_internal_jwt();
            for text in ["", "   "] {
                let res = post_send(&base_url, &jwt, send_body(text)).await;
                assert_eq!(res.status(), 400, "text={text:?}");
                let body: Value = res.json().await.unwrap();
                assert_eq!(body["error"], "text_required");
            }
        }
    );
}

#[tokio::test]
async fn test_send_db_error() {
    test_group!("Internal: send DB error");
    test_case!("channel 取得失敗は 500", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        let mut state = setup_mock_app_state();
        let mock = install_mock(&mut state, Some(sample_bot_cfg()));
        mock.fail_next.store(true, Ordering::SeqCst);
        let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

        let jwt = crate::common::create_test_internal_jwt();
        let res = post_send(&base_url, &jwt, send_body("hello")).await;
        assert_eq!(res.status(), 500);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["message"], "get_failed");
    });
}

#[tokio::test]
async fn test_send_unauthorized() {
    test_group!("Internal: send unauthorized");
    test_case!("JWT 無しは 401 (tenant header では通らない)", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        let mut state = setup_mock_app_state();
        let _mock = install_mock(&mut state, Some(sample_bot_cfg()));
        let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

        let res = reqwest::Client::new()
            .post(format!("{base_url}/api/internal/lineworks/send"))
            .header("X-Tenant-ID", Uuid::new_v4().to_string())
            .json(&send_body("hello"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 401);
    });
}

#[tokio::test]
async fn test_send_user_jwt_rejected() {
    test_group!("Internal: send user JWT");
    test_case!("ユーザー JWT (aud 無し) は 401", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        let mut state = setup_mock_app_state();
        let _mock = install_mock(&mut state, Some(sample_bot_cfg()));
        let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

        let user_jwt = crate::common::create_test_jwt(Uuid::new_v4(), "admin");
        let res = post_send(&base_url, &user_jwt, send_body("hello")).await;
        assert_eq!(res.status(), 401);
    });
}

// ============================================================
// POST /api/internal/lineworks/send — recipient (個人) 宛
// ============================================================

fn recipient_body(text: &str) -> Value {
    serde_json::json!({ "recipient_id": Uuid::new_v4(), "text": text })
}

#[tokio::test]
async fn test_send_recipient_reaches_bot_config_lookup() {
    test_group!("Internal: send to recipient");
    test_case!(
        "recipient は id 引きで tenant ごと解決され、bot_config 不在なら 500",
        {
            let _guard = crate::common::ENV_LOCK.lock().unwrap();
            std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
            let mut state = setup_mock_app_state();
            let _mock = install_mock(&mut state, Some(sample_bot_cfg()));
            let _recipients = install_recipient_mock(&mut state);
            let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

            let jwt = crate::common::create_test_internal_jwt();
            let res = post_send(&base_url, &jwt, recipient_body("予約番号は J5JZPEQJ です")).await;
            // mock の list_configs は空なので bot 選択で止まる = ここまで到達した証拠
            assert_eq!(res.status(), 500);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["message"], "bot_config_not_found");
        }
    );
}

#[tokio::test]
async fn test_send_recipient_not_found() {
    test_group!("Internal: send recipient not found");
    test_case!("未登録 recipient id は 404", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        let mut state = setup_mock_app_state();
        let _mock = install_mock(&mut state, Some(sample_bot_cfg()));
        let recipients = install_recipient_mock(&mut state);
        *recipients.send_recipient.lock().unwrap() = None;
        let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

        let jwt = crate::common::create_test_internal_jwt();
        let res = post_send(&base_url, &jwt, recipient_body("hello")).await;
        assert_eq!(res.status(), 404);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["error"], "recipient_not_found");
    });
}

#[tokio::test]
async fn test_send_recipient_not_lineworks() {
    test_group!("Internal: send recipient not lineworks");
    test_case!(
        "LINE 宛の recipient は 400 (黙って握り潰さない)",
        {
            let _guard = crate::common::ENV_LOCK.lock().unwrap();
            std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
            let mut state = setup_mock_app_state();
            let _mock = install_mock(&mut state, Some(sample_bot_cfg()));
            let recipients = install_recipient_mock(&mut state);
            {
                let mut row = recipients.send_recipient.lock().unwrap();
                let r = row.as_mut().unwrap();
                r.provider = "line".into();
                r.lineworks_user_id = None;
                r.line_user_id = Some("U1234567890".into());
            }
            let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

            let jwt = crate::common::create_test_internal_jwt();
            let res = post_send(&base_url, &jwt, recipient_body("hello")).await;
            assert_eq!(res.status(), 400);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["error"], "recipient_not_lineworks");
        }
    );
}

#[tokio::test]
async fn test_send_recipient_disabled() {
    test_group!("Internal: send recipient disabled");
    test_case!(
        "無効化された recipient は 400 (404 と区別できる形)",
        {
            let _guard = crate::common::ENV_LOCK.lock().unwrap();
            std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
            let mut state = setup_mock_app_state();
            let _mock = install_mock(&mut state, Some(sample_bot_cfg()));
            let recipients = install_recipient_mock(&mut state);
            recipients
                .send_recipient
                .lock()
                .unwrap()
                .as_mut()
                .unwrap()
                .enabled = false;
            let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

            let jwt = crate::common::create_test_internal_jwt();
            let res = post_send(&base_url, &jwt, recipient_body("hello")).await;
            assert_eq!(res.status(), 400);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["error"], "recipient_disabled");
        }
    );
}

#[tokio::test]
async fn test_send_recipient_db_error() {
    test_group!("Internal: send recipient DB error");
    test_case!("recipient 取得失敗は 500", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        let mut state = setup_mock_app_state();
        let _mock = install_mock(&mut state, Some(sample_bot_cfg()));
        let recipients = install_recipient_mock(&mut state);
        recipients.fail_next.store(true, Ordering::SeqCst);
        let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

        let jwt = crate::common::create_test_internal_jwt();
        let res = post_send(&base_url, &jwt, recipient_body("hello")).await;
        assert_eq!(res.status(), 500);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["message"], "get_failed");
    });
}

// ============================================================
// POST /api/internal/lineworks/send — 宛先 2 択の検証
// ============================================================

#[tokio::test]
async fn test_send_rejects_both_targets() {
    test_group!("Internal: send both targets");
    test_case!("channel_id と recipient_id の両方指定は 400", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        let mut state = setup_mock_app_state();
        let _mock = install_mock(&mut state, Some(sample_bot_cfg()));
        let _recipients = install_recipient_mock(&mut state);
        let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

        let jwt = crate::common::create_test_internal_jwt();
        let res = post_send(
            &base_url,
            &jwt,
            serde_json::json!({
                "channel_id": Uuid::new_v4(),
                "recipient_id": Uuid::new_v4(),
                "text": "hello"
            }),
        )
        .await;
        assert_eq!(res.status(), 400);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["error"], "target_ambiguous");
    });
}

#[tokio::test]
async fn test_send_rejects_no_target() {
    test_group!("Internal: send no target");
    test_case!("宛先をどちらも省略すると 400", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        let mut state = setup_mock_app_state();
        let _mock = install_mock(&mut state, Some(sample_bot_cfg()));
        let _recipients = install_recipient_mock(&mut state);
        let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

        let jwt = crate::common::create_test_internal_jwt();
        let res = post_send(&base_url, &jwt, serde_json::json!({ "text": "hello" })).await;
        assert_eq!(res.status(), 400);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["error"], "target_required");
    });
}

#[tokio::test]
async fn test_send_explicit_null_target_is_unspecified() {
    test_group!("Internal: send explicit null target");
    test_case!(
        "キー有り値 null は「未指定」— もう片方だけ指定と同じに通る",
        {
            let _guard = crate::common::ENV_LOCK.lock().unwrap();
            std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
            let mut state = setup_mock_app_state();
            let _mock = install_mock(&mut state, Some(sample_bot_cfg()));
            let _recipients = install_recipient_mock(&mut state);
            let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

            let jwt = crate::common::create_test_internal_jwt();
            // channel_id: null + recipient_id 指定 → recipient 経路へ進む
            // (400 target_ambiguous にならず、bot 選択まで到達する)
            let res = post_send(
                &base_url,
                &jwt,
                serde_json::json!({
                    "channel_id": null,
                    "recipient_id": Uuid::new_v4(),
                    "text": "hello"
                }),
            )
            .await;
            assert_eq!(res.status(), 500);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["message"], "bot_config_not_found");

            // 両方 null は「両方省略」と同じ 400 target_required
            let res = post_send(
                &base_url,
                &jwt,
                serde_json::json!({ "channel_id": null, "recipient_id": null, "text": "hello" }),
            )
            .await;
            assert_eq!(res.status(), 400);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["error"], "target_required");
        }
    );
}

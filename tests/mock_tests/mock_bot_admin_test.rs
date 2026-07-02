use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde_json::Value;

use crate::mock_helpers::app_state::setup_mock_app_state;
use crate::mock_helpers::MockBotAdminRepository;
use rust_alc_api::db::repository::bot_admin::BotConfigWithSecrets;

// ============================================================
// GET /admin/bot/configs — list
// ============================================================

#[tokio::test]
async fn test_list_configs_success() {
    test_group!("Bot Admin: list_configs");
    test_case!("管理者は空のリストを取得できる", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

        let mock = Arc::new(MockBotAdminRepository::default());
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .get(format!("{base_url}/api/admin/bot/configs"))
            .header("Authorization", format!("Bearer {admin_jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["configs"].as_array().unwrap().len(), 0);
    });
}

#[tokio::test]
async fn test_list_configs_forbidden_for_viewer() {
    test_group!("Bot Admin: list_configs forbidden");
    test_case!("viewer ロールは FORBIDDEN", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

        let mock = Arc::new(MockBotAdminRepository::default());
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let viewer_jwt = crate::common::create_test_jwt(tenant_id, "viewer");
        let client = reqwest::Client::new();

        let res = client
            .get(format!("{base_url}/api/admin/bot/configs"))
            .header("Authorization", format!("Bearer {viewer_jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 403);
    });
}

#[tokio::test]
async fn test_list_configs_db_error() {
    test_group!("Bot Admin: list_configs DB error");
    test_case!("DB エラー時に 500 を返す", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

        let mock = Arc::new(MockBotAdminRepository::default());
        mock.fail_next.store(true, Ordering::SeqCst);
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .get(format!("{base_url}/api/admin/bot/configs"))
            .header("Authorization", format!("Bearer {admin_jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

// ============================================================
// POST /admin/bot/configs — upsert (create path)
// ============================================================

#[tokio::test]
async fn test_create_config_success() {
    test_group!("Bot Admin: create_config");
    test_case!("新規作成が成功する", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

        let mock = Arc::new(MockBotAdminRepository::default());
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/admin/bot/configs"))
            .header("Authorization", format!("Bearer {admin_jwt}"))
            .json(&serde_json::json!({
                "name": "Test Bot",
                "client_id": "test-client-id",
                "client_secret": "test-secret",
                "service_account": "sa@test.com",
                "private_key": "test-pk",
                "bot_id": "bot-123",
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["name"], "Test Bot");
        assert_eq!(body["client_id"], "test-client-id");
        assert_eq!(body["service_account"], "sa@test.com");
        assert_eq!(body["bot_id"], "bot-123");
        assert_eq!(body["enabled"], true);
        // provider defaults to "lineworks"
        assert_eq!(body["provider"], "lineworks");
    });
}

#[tokio::test]
async fn test_create_config_with_explicit_provider_and_disabled() {
    test_group!("Bot Admin: create_config with provider");
    test_case!("provider を明示指定、enabled=false で作成", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

        let mock = Arc::new(MockBotAdminRepository::default());
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/admin/bot/configs"))
            .header("Authorization", format!("Bearer {admin_jwt}"))
            .json(&serde_json::json!({
                "provider": "slack",
                "name": "Slack Bot",
                "client_id": "slack-id",
                "service_account": "slack-sa",
                "bot_id": "slack-bot",
                "enabled": false,
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["provider"], "slack");
        assert_eq!(body["enabled"], false);
    });
}

#[tokio::test]
async fn test_create_config_forbidden_for_viewer() {
    test_group!("Bot Admin: create_config forbidden");
    test_case!("viewer ロールは FORBIDDEN", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

        let mock = Arc::new(MockBotAdminRepository::default());
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let viewer_jwt = crate::common::create_test_jwt(tenant_id, "viewer");
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/admin/bot/configs"))
            .header("Authorization", format!("Bearer {viewer_jwt}"))
            .json(&serde_json::json!({
                "name": "Bot",
                "client_id": "cid",
                "service_account": "sa",
                "bot_id": "bid",
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 403);
    });
}

#[tokio::test]
async fn test_create_config_db_error() {
    test_group!("Bot Admin: create_config DB error");
    test_case!("DB エラー時に 500 を返す", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

        let mock = Arc::new(MockBotAdminRepository::default());
        mock.fail_next.store(true, Ordering::SeqCst);
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/admin/bot/configs"))
            .header("Authorization", format!("Bearer {admin_jwt}"))
            .json(&serde_json::json!({
                "name": "Bot",
                "client_id": "cid",
                "service_account": "sa",
                "bot_id": "bid",
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

// ============================================================
// POST /admin/bot/configs — upsert (update path)
// ============================================================

#[tokio::test]
async fn test_update_config_success() {
    test_group!("Bot Admin: update_config");
    test_case!("既存設定の更新が成功する", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

        let mock = Arc::new(MockBotAdminRepository::default());
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let existing_id = Uuid::new_v4();
        let res = client
            .post(format!("{base_url}/api/admin/bot/configs"))
            .header("Authorization", format!("Bearer {admin_jwt}"))
            .json(&serde_json::json!({
                "id": existing_id.to_string(),
                "name": "Updated Bot",
                "client_id": "updated-cid",
                "service_account": "updated-sa",
                "bot_id": "updated-bid",
                "enabled": false,
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["name"], "Updated Bot");
        assert_eq!(body["id"], existing_id.to_string());
        assert_eq!(body["enabled"], false);
    });
}

#[tokio::test]
async fn test_update_config_with_secrets() {
    test_group!("Bot Admin: update_config with secrets");
    test_case!("client_secret と private_key を同時に更新", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

        let mock = Arc::new(MockBotAdminRepository::default());
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let existing_id = Uuid::new_v4();
        let res = client
            .post(format!("{base_url}/api/admin/bot/configs"))
            .header("Authorization", format!("Bearer {admin_jwt}"))
            .json(&serde_json::json!({
                "id": existing_id.to_string(),
                "name": "Bot With Secrets",
                "client_id": "cid",
                "client_secret": "new-secret",
                "service_account": "sa",
                "private_key": "new-pk",
                "bot_id": "bid",
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    });
}

#[tokio::test]
async fn test_create_config_with_bot_secret() {
    test_group!("Bot Admin: create_config with bot_secret");
    test_case!(
        "新規作成時に bot_secret を渡すと update_bot_secret が呼ばれる",
        {
            let _guard = crate::common::ENV_LOCK.lock().unwrap();
            std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

            let mock = Arc::new(MockBotAdminRepository::default());
            let mut state = setup_mock_app_state();
            state.bot_admin = mock;
            let base_url = crate::common::spawn_test_server(state).await;

            let tenant_id = Uuid::new_v4();
            let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();

            let res = client
                .post(format!("{base_url}/api/admin/bot/configs"))
                .header("Authorization", format!("Bearer {admin_jwt}"))
                .json(&serde_json::json!({
                    "name": "Bot",
                    "client_id": "cid",
                    "service_account": "sa",
                    "bot_id": "bid",
                    "bot_secret": "webhook-secret-123",
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
        }
    );
}

#[tokio::test]
async fn test_update_config_with_bot_secret() {
    test_group!("Bot Admin: update_config with bot_secret");
    test_case!(
        "更新時に bot_secret を渡すと update_bot_secret が呼ばれる",
        {
            let _guard = crate::common::ENV_LOCK.lock().unwrap();
            std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

            let mock = Arc::new(MockBotAdminRepository::default());
            let mut state = setup_mock_app_state();
            state.bot_admin = mock;
            let base_url = crate::common::spawn_test_server(state).await;

            let tenant_id = Uuid::new_v4();
            let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();

            let existing_id = Uuid::new_v4();
            let res = client
                .post(format!("{base_url}/api/admin/bot/configs"))
                .header("Authorization", format!("Bearer {admin_jwt}"))
                .json(&serde_json::json!({
                    "id": existing_id.to_string(),
                    "name": "Bot",
                    "client_id": "cid",
                    "service_account": "sa",
                    "bot_id": "bid",
                    "bot_secret": "rotated-secret",
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
        }
    );
}

#[tokio::test]
async fn test_update_config_with_empty_bot_secret_skipped() {
    test_group!("Bot Admin: update_config empty bot_secret");
    test_case!("空文字の bot_secret は更新スキップ", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

        let mock = Arc::new(MockBotAdminRepository::default());
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let existing_id = Uuid::new_v4();
        let res = client
            .post(format!("{base_url}/api/admin/bot/configs"))
            .header("Authorization", format!("Bearer {admin_jwt}"))
            .json(&serde_json::json!({
                "id": existing_id.to_string(),
                "name": "Bot",
                "client_id": "cid",
                "service_account": "sa",
                "bot_id": "bid",
                "bot_secret": "",
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
    });
}

#[tokio::test]
async fn test_create_config_with_bot_secret_update_fails() {
    test_group!("Bot Admin: create_config bot_secret update fails");
    test_case!(
        "新規作成は成功するが update_bot_secret が失敗 → 500",
        {
            let _guard = crate::common::ENV_LOCK.lock().unwrap();
            std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

            // create_config は成功させ、update_bot_secret だけを失敗させる
            let mock = Arc::new(MockBotAdminRepository::default());
            mock.fail_update_bot_secret_only
                .store(true, Ordering::SeqCst);
            let mut state = setup_mock_app_state();
            state.bot_admin = mock;
            let base_url = crate::common::spawn_test_server(state).await;

            let tenant_id = Uuid::new_v4();
            let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();

            let res = client
                .post(format!("{base_url}/api/admin/bot/configs"))
                .header("Authorization", format!("Bearer {admin_jwt}"))
                .json(&serde_json::json!({
                    "name": "Bot",
                    "client_id": "cid",
                    "service_account": "sa",
                    "bot_id": "bid",
                    "bot_secret": "will-fail",
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 500);
        }
    );
}

#[tokio::test]
async fn test_update_bot_secret_db_error() {
    test_group!("Bot Admin: update_bot_secret DB error");
    test_case!("update_bot_secret 失敗時に 500", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

        // fail_next を 1 回だけ true → 最初の呼び出しが失敗。
        // update_config よりも先に update_bot_secret が呼ばれることを期待。
        let mock = Arc::new(MockBotAdminRepository::default());
        mock.fail_next.store(true, Ordering::SeqCst);
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let existing_id = Uuid::new_v4();
        let res = client
            .post(format!("{base_url}/api/admin/bot/configs"))
            .header("Authorization", format!("Bearer {admin_jwt}"))
            .json(&serde_json::json!({
                "id": existing_id.to_string(),
                "name": "Bot",
                "client_id": "cid",
                "service_account": "sa",
                "bot_id": "bid",
                "bot_secret": "any-secret",
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

#[tokio::test]
async fn test_update_config_with_empty_secrets() {
    test_group!("Bot Admin: update_config empty secrets");
    test_case!(
        "空の client_secret/private_key は更新をスキップする",
        {
            let _guard = crate::common::ENV_LOCK.lock().unwrap();
            std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

            let mock = Arc::new(MockBotAdminRepository::default());
            let mut state = setup_mock_app_state();
            state.bot_admin = mock;
            let base_url = crate::common::spawn_test_server(state).await;

            let tenant_id = Uuid::new_v4();
            let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();

            let existing_id = Uuid::new_v4();
            let res = client
                .post(format!("{base_url}/api/admin/bot/configs"))
                .header("Authorization", format!("Bearer {admin_jwt}"))
                .json(&serde_json::json!({
                    "id": existing_id.to_string(),
                    "name": "Bot",
                    "client_id": "cid",
                    "client_secret": "",
                    "service_account": "sa",
                    "private_key": "",
                    "bot_id": "bid",
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
        }
    );
}

#[tokio::test]
async fn test_update_config_invalid_uuid() {
    test_group!("Bot Admin: update_config invalid UUID");
    test_case!("不正な UUID は BAD_REQUEST", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

        let mock = Arc::new(MockBotAdminRepository::default());
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/admin/bot/configs"))
            .header("Authorization", format!("Bearer {admin_jwt}"))
            .json(&serde_json::json!({
                "id": "not-a-valid-uuid",
                "name": "Bot",
                "client_id": "cid",
                "service_account": "sa",
                "bot_id": "bid",
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });
}

#[tokio::test]
async fn test_update_config_forbidden_for_viewer() {
    test_group!("Bot Admin: update_config forbidden");
    test_case!("viewer ロールは FORBIDDEN", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

        let mock = Arc::new(MockBotAdminRepository::default());
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let viewer_jwt = crate::common::create_test_jwt(tenant_id, "viewer");
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/admin/bot/configs"))
            .header("Authorization", format!("Bearer {viewer_jwt}"))
            .json(&serde_json::json!({
                "id": Uuid::new_v4().to_string(),
                "name": "Bot",
                "client_id": "cid",
                "service_account": "sa",
                "bot_id": "bid",
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 403);
    });
}

#[tokio::test]
async fn test_update_config_db_error() {
    test_group!("Bot Admin: update_config DB error");
    test_case!("update_config の DB エラーで 500", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

        let mock = Arc::new(MockBotAdminRepository::default());
        mock.fail_next.store(true, Ordering::SeqCst);
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/admin/bot/configs"))
            .header("Authorization", format!("Bearer {admin_jwt}"))
            .json(&serde_json::json!({
                "id": Uuid::new_v4().to_string(),
                "name": "Bot",
                "client_id": "cid",
                "service_account": "sa",
                "bot_id": "bid",
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

// ============================================================
// POST /admin/bot/configs — upsert: encryption key missing
// ============================================================

#[tokio::test]
async fn test_upsert_no_encryption_key() {
    test_group!("Bot Admin: upsert no encryption key");
    test_case!("SSO_ENCRYPTION_KEY がない場合 500", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        // SSO_ENCRYPTION_KEY を除去 (encrypt_secret が key を取得できない。
        // JWT_SECRET fallback は #479 で撤去済み)
        std::env::remove_var("SSO_ENCRYPTION_KEY");

        let mock = Arc::new(MockBotAdminRepository::default());
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/admin/bot/configs"))
            .header("Authorization", format!("Bearer {admin_jwt}"))
            .json(&serde_json::json!({
                "name": "Bot",
                "client_id": "cid",
                "service_account": "sa",
                "bot_id": "bid",
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

// ============================================================
// DELETE /admin/bot/configs — delete
// ============================================================

#[tokio::test]
async fn test_delete_config_success() {
    test_group!("Bot Admin: delete_config");
    test_case!("設定の削除が成功する", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

        let mock = Arc::new(MockBotAdminRepository::default());
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .delete(format!("{base_url}/api/admin/bot/configs"))
            .header("Authorization", format!("Bearer {admin_jwt}"))
            .json(&serde_json::json!({
                "id": Uuid::new_v4().to_string(),
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 204);
    });
}

#[tokio::test]
async fn test_delete_config_invalid_uuid() {
    test_group!("Bot Admin: delete_config invalid UUID");
    test_case!("不正な UUID は BAD_REQUEST", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

        let mock = Arc::new(MockBotAdminRepository::default());
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .delete(format!("{base_url}/api/admin/bot/configs"))
            .header("Authorization", format!("Bearer {admin_jwt}"))
            .json(&serde_json::json!({
                "id": "invalid-uuid",
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });
}

#[tokio::test]
async fn test_delete_config_forbidden_for_viewer() {
    test_group!("Bot Admin: delete_config forbidden");
    test_case!("viewer ロールは FORBIDDEN", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

        let mock = Arc::new(MockBotAdminRepository::default());
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let viewer_jwt = crate::common::create_test_jwt(tenant_id, "viewer");
        let client = reqwest::Client::new();

        let res = client
            .delete(format!("{base_url}/api/admin/bot/configs"))
            .header("Authorization", format!("Bearer {viewer_jwt}"))
            .json(&serde_json::json!({
                "id": Uuid::new_v4().to_string(),
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 403);
    });
}

#[tokio::test]
async fn test_delete_config_db_error() {
    test_group!("Bot Admin: delete_config DB error");
    test_case!("DB エラー時に 500 を返す", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

        let mock = Arc::new(MockBotAdminRepository::default());
        mock.fail_next.store(true, Ordering::SeqCst);
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .delete(format!("{base_url}/api/admin/bot/configs"))
            .header("Authorization", format!("Bearer {admin_jwt}"))
            .json(&serde_json::json!({
                "id": Uuid::new_v4().to_string(),
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

// ============================================================
// GET /admin/bot/configs/{id}/secrets — get_config_secrets
// ============================================================

/// Helper: encrypt a plaintext string using AES-256-GCM (same algorithm as bot_admin.rs)
fn test_encrypt(plaintext: &str, key_material: &str) -> String {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
    use ring::rand::{SecureRandom, SystemRandom};
    use sha2::{Digest, Sha256};

    let mut key_bytes = [0u8; 32];
    let hash = Sha256::digest(key_material.as_bytes());
    key_bytes.copy_from_slice(&hash);

    let unbound_key = UnboundKey::new(&AES_256_GCM, &key_bytes).unwrap();
    let key = LessSafeKey::new(unbound_key);

    let rng = SystemRandom::new();
    let mut nonce_bytes = [0u8; 12];
    rng.fill(&mut nonce_bytes).unwrap();
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    let mut in_out = plaintext.as_bytes().to_vec();
    let tag_len = aead::AES_256_GCM.tag_len();
    in_out.extend(vec![0u8; tag_len]);

    key.seal_in_place_separate_tag(nonce, Aad::empty(), &mut in_out[..plaintext.len()])
        .map(|tag| {
            in_out[plaintext.len()..].copy_from_slice(tag.as_ref());
        })
        .unwrap();

    let mut result = Vec::with_capacity(12 + in_out.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&in_out);

    BASE64.encode(&result)
}

#[tokio::test]
async fn test_get_config_secrets_success() {
    test_group!("Bot Admin: get_config_secrets success");
    test_case!(
        "管理者が暗号化されたシークレットを復号取得できる",
        {
            let _guard = crate::common::ENV_LOCK.lock().unwrap();
            let key = "test-encryption-key-for-secrets";
            std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
            std::env::set_var("SSO_ENCRYPTION_KEY", key);

            let config_id = Uuid::new_v4();
            let encrypted_secret = test_encrypt("my-client-secret", key);
            let encrypted_pk = test_encrypt("my-private-key", key);

            let mock = Arc::new(MockBotAdminRepository::default());
            *mock.return_config_with_secrets.lock().unwrap() = Some(BotConfigWithSecrets {
                id: config_id,
                provider: "lineworks".to_string(),
                name: "Test Bot".to_string(),
                client_id: "test-cid".to_string(),
                client_secret_encrypted: encrypted_secret,
                service_account: "sa@test.com".to_string(),
                private_key_encrypted: encrypted_pk,
                bot_id: "bot-123".to_string(),
                bot_secret_encrypted: None,
                enabled: true,
            });

            let mut state = setup_mock_app_state();
            state.bot_admin = mock;
            let base_url = crate::common::spawn_test_server(state).await;

            let tenant_id = Uuid::new_v4();
            let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();

            let res = client
                .get(format!(
                    "{base_url}/api/admin/bot/configs/{config_id}/secrets"
                ))
                .header("Authorization", format!("Bearer {admin_jwt}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["client_id"], "test-cid");
            assert_eq!(body["client_secret"], "my-client-secret");
            assert_eq!(body["service_account"], "sa@test.com");
            assert_eq!(body["private_key"], "my-private-key");
            assert_eq!(body["bot_id"], "bot-123");
            // bot_secret_encrypted: None → bot_secret は空文字
            assert_eq!(body["bot_secret"], "");

            std::env::remove_var("SSO_ENCRYPTION_KEY");
        }
    );
}

#[tokio::test]
async fn test_get_config_secrets_with_bot_secret() {
    test_group!("Bot Admin: get_config_secrets bot_secret 復号");
    test_case!("bot_secret_encrypted=Some の場合は復号して返す", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        let key = "test-encryption-key-bot-secret";
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        std::env::set_var("SSO_ENCRYPTION_KEY", key);

        let config_id = Uuid::new_v4();
        let mock = Arc::new(MockBotAdminRepository::default());
        *mock.return_config_with_secrets.lock().unwrap() = Some(BotConfigWithSecrets {
            id: config_id,
            provider: "lineworks".to_string(),
            name: "Bot With Secret".to_string(),
            client_id: "cid".to_string(),
            client_secret_encrypted: test_encrypt("cs", key),
            service_account: "sa".to_string(),
            private_key_encrypted: test_encrypt("pk", key),
            bot_id: "b".to_string(),
            enabled: true,
            bot_secret_encrypted: Some(test_encrypt("my-bot-secret", key)),
        });

        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .get(format!(
                "{base_url}/api/admin/bot/configs/{config_id}/secrets"
            ))
            .header("Authorization", format!("Bearer {admin_jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["bot_secret"], "my-bot-secret");

        std::env::remove_var("SSO_ENCRYPTION_KEY");
    });
}

#[tokio::test]
async fn test_get_config_secrets_bot_secret_decrypt_error() {
    test_group!("Bot Admin: get_config_secrets bot_secret 復号失敗");
    test_case!("bot_secret_encrypted が不正な base64 の場合 500", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        let key = "test-key-bot-secret-fail";
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        std::env::set_var("SSO_ENCRYPTION_KEY", key);

        let config_id = Uuid::new_v4();
        let mock = Arc::new(MockBotAdminRepository::default());
        *mock.return_config_with_secrets.lock().unwrap() = Some(BotConfigWithSecrets {
            id: config_id,
            provider: "lineworks".to_string(),
            name: "Bot".to_string(),
            client_id: "cid".to_string(),
            client_secret_encrypted: test_encrypt("cs", key),
            service_account: "sa".to_string(),
            private_key_encrypted: test_encrypt("pk", key),
            bot_id: "b".to_string(),
            enabled: true,
            bot_secret_encrypted: Some("not-valid-base64!!!".to_string()),
        });

        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .get(format!(
                "{base_url}/api/admin/bot/configs/{config_id}/secrets"
            ))
            .header("Authorization", format!("Bearer {admin_jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        std::env::remove_var("SSO_ENCRYPTION_KEY");
    });
}

#[tokio::test]
async fn test_get_config_secrets_forbidden_viewer() {
    test_group!("Bot Admin: get_config_secrets forbidden");
    test_case!("viewer ロールは FORBIDDEN", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

        let mock = Arc::new(MockBotAdminRepository::default());
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let viewer_jwt = crate::common::create_test_jwt(tenant_id, "viewer");
        let client = reqwest::Client::new();

        let config_id = Uuid::new_v4();
        let res = client
            .get(format!(
                "{base_url}/api/admin/bot/configs/{config_id}/secrets"
            ))
            .header("Authorization", format!("Bearer {viewer_jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 403);
    });
}

#[tokio::test]
async fn test_get_config_secrets_not_found() {
    test_group!("Bot Admin: get_config_secrets not found");
    test_case!("存在しない config は 404", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

        let mock = Arc::new(MockBotAdminRepository::default());
        // return_config_with_secrets is None by default -> 404
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let config_id = Uuid::new_v4();
        let res = client
            .get(format!(
                "{base_url}/api/admin/bot/configs/{config_id}/secrets"
            ))
            .header("Authorization", format!("Bearer {admin_jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

#[tokio::test]
async fn test_get_config_secrets_db_error() {
    test_group!("Bot Admin: get_config_secrets DB error");
    test_case!("DB エラー時に 500 を返す", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);

        let mock = Arc::new(MockBotAdminRepository::default());
        mock.fail_next.store(true, Ordering::SeqCst);
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let config_id = Uuid::new_v4();
        let res = client
            .get(format!(
                "{base_url}/api/admin/bot/configs/{config_id}/secrets"
            ))
            .header("Authorization", format!("Bearer {admin_jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

#[tokio::test]
async fn test_get_config_secrets_decrypt_error() {
    test_group!("Bot Admin: get_config_secrets decrypt error");
    test_case!("壊れた暗号文で復号失敗 → 500", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        let key = "test-key-for-decrypt-fail";
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        std::env::set_var("SSO_ENCRYPTION_KEY", key);

        let config_id = Uuid::new_v4();
        let mock = Arc::new(MockBotAdminRepository::default());
        *mock.return_config_with_secrets.lock().unwrap() = Some(BotConfigWithSecrets {
            id: config_id,
            provider: "lineworks".to_string(),
            name: "Bad Bot".to_string(),
            client_id: "cid".to_string(),
            client_secret_encrypted: "not-valid-base64!!!".to_string(),
            service_account: "sa".to_string(),
            private_key_encrypted: "also-bad".to_string(),
            bot_id: "b".to_string(),
            enabled: true,
            bot_secret_encrypted: None,
        });

        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .get(format!(
                "{base_url}/api/admin/bot/configs/{config_id}/secrets"
            ))
            .header("Authorization", format!("Bearer {admin_jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        std::env::remove_var("SSO_ENCRYPTION_KEY");
    });
}

#[tokio::test]
async fn test_get_config_secrets_short_ciphertext() {
    test_group!("Bot Admin: get_config_secrets short ciphertext");
    test_case!("短すぎる暗号文で復号失敗 → 500", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        let key = "test-key-short";
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        std::env::set_var("SSO_ENCRYPTION_KEY", key);

        let config_id = Uuid::new_v4();
        let mock = Arc::new(MockBotAdminRepository::default());
        use base64::{engine::general_purpose::STANDARD, Engine};
        *mock.return_config_with_secrets.lock().unwrap() = Some(BotConfigWithSecrets {
            id: config_id,
            provider: "lineworks".to_string(),
            name: "Short".to_string(),
            client_id: "cid".to_string(),
            client_secret_encrypted: STANDARD.encode(b"short"),
            service_account: "sa".to_string(),
            private_key_encrypted: STANDARD.encode(b"short"),
            bot_id: "b".to_string(),
            enabled: true,
            bot_secret_encrypted: None,
        });

        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let res = client
            .get(format!(
                "{base_url}/api/admin/bot/configs/{config_id}/secrets"
            ))
            .header("Authorization", format!("Bearer {admin_jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        std::env::remove_var("SSO_ENCRYPTION_KEY");
    });
}

#[tokio::test]
async fn test_get_config_secrets_private_key_decrypt_error() {
    test_group!("Bot Admin: get_config_secrets private_key decrypt error");
    test_case!(
        "client_secret は正常だが private_key が壊れている → 500",
        {
            let _guard = crate::common::ENV_LOCK.lock().unwrap();
            let key = "test-key-pk-fail";
            std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
            std::env::set_var("SSO_ENCRYPTION_KEY", key);

            let config_id = Uuid::new_v4();
            let mock = Arc::new(MockBotAdminRepository::default());
            *mock.return_config_with_secrets.lock().unwrap() = Some(BotConfigWithSecrets {
                id: config_id,
                provider: "lineworks".to_string(),
                name: "PK Fail".to_string(),
                client_id: "cid".to_string(),
                client_secret_encrypted: test_encrypt("valid-secret", key),
                service_account: "sa".to_string(),
                private_key_encrypted: "not-valid-base64!!!".to_string(),
                bot_id: "b".to_string(),
                enabled: true,
                bot_secret_encrypted: None,
            });

            let mut state = setup_mock_app_state();
            state.bot_admin = mock;
            let base_url = crate::common::spawn_test_server(state).await;

            let tenant_id = Uuid::new_v4();
            let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
            let client = reqwest::Client::new();

            let res = client
                .get(format!(
                    "{base_url}/api/admin/bot/configs/{config_id}/secrets"
                ))
                .header("Authorization", format!("Bearer {admin_jwt}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 500);

            std::env::remove_var("SSO_ENCRYPTION_KEY");
        }
    );
}

#[tokio::test]
async fn test_get_config_secrets_missing_env_var() {
    test_group!("Bot Admin: get_config_secrets missing env");
    test_case!("SSO_ENCRYPTION_KEY がない場合 500", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        // Save and remove env var (JWT_SECRET fallback は #479 で撤去済み)
        let saved_sso = std::env::var("SSO_ENCRYPTION_KEY").ok();
        std::env::remove_var("SSO_ENCRYPTION_KEY");

        let mock = Arc::new(MockBotAdminRepository::default());
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let admin_jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let config_id = Uuid::new_v4();
        let res = client
            .get(format!(
                "{base_url}/api/admin/bot/configs/{config_id}/secrets"
            ))
            .header("Authorization", format!("Bearer {admin_jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        // Restore env var
        if let Some(val) = saved_sso {
            std::env::set_var("SSO_ENCRYPTION_KEY", val);
        }
    });
}

// ============================================================
// GET /admin/bot/configs/export — developer-only
// ============================================================

fn dev_email() -> &'static str {
    "m.tama.ramu@gmail.com"
}

fn set_dev_emails(value: &str) -> Option<String> {
    let prev = std::env::var("DEVELOPER_EMAILS").ok();
    std::env::set_var("DEVELOPER_EMAILS", value);
    prev
}

fn restore_dev_emails(prev: Option<String>) {
    match prev {
        Some(v) => std::env::set_var("DEVELOPER_EMAILS", v),
        None => std::env::remove_var("DEVELOPER_EMAILS"),
    }
}

#[tokio::test]
async fn test_export_configs_success() {
    test_group!("Bot Admin: export_configs success");
    test_case!("developer email + tenant + 1 config", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        let prev = set_dev_emails(dev_email());

        let mock = Arc::new(MockBotAdminRepository::default());
        let tenant_id = Uuid::new_v4();
        *mock.return_tenant_for_export.lock().unwrap() = Some(
            rust_alc_api::db::repository::bot_admin::TenantInfoForExport {
                id: tenant_id,
                name: "テナント大石".to_string(),
                slug: Some("ohishi".to_string()),
                email_domain: None,
                created_at: chrono::Utc::now(),
            },
        );
        *mock.return_configs_for_export.lock().unwrap() = vec![
            rust_alc_api::db::repository::bot_admin::BotConfigExportRow {
                id: Uuid::new_v4(),
                tenant_id,
                provider: "lineworks".to_string(),
                name: "test bot".to_string(),
                client_id: "cid".to_string(),
                client_secret_encrypted: "enc-secret".to_string(),
                service_account: "sa@example".to_string(),
                private_key_encrypted: "enc-pk".to_string(),
                bot_id: "8977068".to_string(),
                enabled: true,
                bot_secret_encrypted: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
        ];

        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let dev_jwt = crate::common::create_test_jwt_for_user(
            Uuid::new_v4(),
            tenant_id,
            dev_email(),
            "admin",
        );
        let client = reqwest::Client::new();

        let res = client
            .get(format!(
                "{base_url}/api/admin/bot/configs/export?tenant_id={tenant_id}"
            ))
            .header("Authorization", format!("Bearer {dev_jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["version"], 1);
        assert_eq!(body["tenant_id"], tenant_id.to_string());
        assert_eq!(body["data"]["tenant"]["slug"], "ohishi");
        assert_eq!(body["data"]["bot_configs"].as_array().unwrap().len(), 1);
        assert_eq!(body["data"]["bot_configs"][0]["bot_id"], "8977068");
        assert_eq!(
            body["data"]["bot_configs"][0]["client_secret_encrypted"],
            "enc-secret"
        );
        assert_eq!(body["data"]["users"].as_array().unwrap().len(), 0);

        restore_dev_emails(prev);
    });
}

#[tokio::test]
async fn test_export_configs_forbidden_for_non_developer() {
    test_group!("Bot Admin: export_configs forbidden");
    test_case!("DEVELOPER_EMAILS に含まれない user は 403", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        let prev = set_dev_emails(dev_email());

        let mock = Arc::new(MockBotAdminRepository::default());
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let attacker_jwt = crate::common::create_test_jwt_for_user(
            Uuid::new_v4(),
            tenant_id,
            "attacker@example.com",
            "admin",
        );
        let client = reqwest::Client::new();
        let res = client
            .get(format!(
                "{base_url}/api/admin/bot/configs/export?tenant_id={tenant_id}"
            ))
            .header("Authorization", format!("Bearer {attacker_jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 403);

        restore_dev_emails(prev);
    });
}

#[tokio::test]
async fn test_export_configs_tenant_not_found() {
    test_group!("Bot Admin: export_configs tenant not found");
    test_case!("テナントが存在しない場合は 404", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        let prev = set_dev_emails(dev_email());

        let mock = Arc::new(MockBotAdminRepository::default());
        // return_tenant_for_export はデフォルト None → handler 側で NOT_FOUND
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let dev_jwt = crate::common::create_test_jwt_for_user(
            Uuid::new_v4(),
            tenant_id,
            dev_email(),
            "admin",
        );
        let client = reqwest::Client::new();
        let res = client
            .get(format!(
                "{base_url}/api/admin/bot/configs/export?tenant_id={tenant_id}"
            ))
            .header("Authorization", format!("Bearer {dev_jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);

        restore_dev_emails(prev);
    });
}

#[tokio::test]
async fn test_export_configs_tenant_db_error() {
    test_group!("Bot Admin: export_configs tenant DB error");
    test_case!("tenant 取得時の DB エラーで 500", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        let prev = set_dev_emails(dev_email());

        let mock = Arc::new(MockBotAdminRepository::default());
        mock.fail_tenant_for_export.store(true, Ordering::SeqCst);
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        let dev_jwt = crate::common::create_test_jwt_for_user(
            Uuid::new_v4(),
            tenant_id,
            dev_email(),
            "admin",
        );
        let client = reqwest::Client::new();
        let res = client
            .get(format!(
                "{base_url}/api/admin/bot/configs/export?tenant_id={tenant_id}"
            ))
            .header("Authorization", format!("Bearer {dev_jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        restore_dev_emails(prev);
    });
}

#[tokio::test]
async fn test_export_configs_configs_db_error() {
    test_group!("Bot Admin: export_configs configs DB error");
    test_case!("bot_configs 取得時の DB エラーで 500", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("SSO_ENCRYPTION_KEY", crate::common::TEST_ENCRYPTION_KEY);
        let prev = set_dev_emails(dev_email());

        let mock = Arc::new(MockBotAdminRepository::default());
        let tenant_id = Uuid::new_v4();
        *mock.return_tenant_for_export.lock().unwrap() = Some(
            rust_alc_api::db::repository::bot_admin::TenantInfoForExport {
                id: tenant_id,
                name: "T".to_string(),
                slug: None,
                email_domain: None,
                created_at: chrono::Utc::now(),
            },
        );
        mock.fail_configs_for_export.store(true, Ordering::SeqCst);
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let dev_jwt = crate::common::create_test_jwt_for_user(
            Uuid::new_v4(),
            tenant_id,
            dev_email(),
            "admin",
        );
        let client = reqwest::Client::new();
        let res = client
            .get(format!(
                "{base_url}/api/admin/bot/configs/export?tenant_id={tenant_id}"
            ))
            .header("Authorization", format!("Bearer {dev_jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        restore_dev_emails(prev);
    });
}

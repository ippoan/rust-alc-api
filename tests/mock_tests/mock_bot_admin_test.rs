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
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
async fn test_update_config_with_empty_secrets() {
    test_group!("Bot Admin: update_config empty secrets");
    test_case!(
        "空の client_secret/private_key は更新をスキップする",
        {
            let _guard = crate::common::ENV_LOCK.lock().unwrap();
            std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
    test_case!("SSO_ENCRYPTION_KEY も JWT_SECRET もない場合 500", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        // JWT_SECRET を一時的に除去 (encrypt_secret が key を取得できない)
        let saved = std::env::var("JWT_SECRET").ok();
        std::env::remove_var("JWT_SECRET");
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

        // 環境変数を復元
        if let Some(val) = saved {
            std::env::set_var("JWT_SECRET", val);
        }
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
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
            std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
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

            std::env::remove_var("SSO_ENCRYPTION_KEY");
        }
    );
}

#[tokio::test]
async fn test_get_config_secrets_forbidden_viewer() {
    test_group!("Bot Admin: get_config_secrets forbidden");
    test_case!("viewer ロールは FORBIDDEN", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);

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
async fn test_get_config_secrets_missing_env_var() {
    test_group!("Bot Admin: get_config_secrets missing env");
    test_case!("SSO_ENCRYPTION_KEY も JWT_SECRET もない場合 500", {
        let _guard = crate::common::ENV_LOCK.lock().unwrap();
        // Save and remove both env vars
        let saved_jwt = std::env::var("JWT_SECRET").ok();
        let saved_sso = std::env::var("SSO_ENCRYPTION_KEY").ok();
        std::env::remove_var("JWT_SECRET");
        std::env::remove_var("SSO_ENCRYPTION_KEY");

        let mock = Arc::new(MockBotAdminRepository::default());
        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::common::spawn_test_server(state).await;

        let tenant_id = Uuid::new_v4();
        // create_test_jwt uses TEST_JWT_SECRET constant directly
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

        // Restore env vars
        if let Some(val) = saved_jwt {
            std::env::set_var("JWT_SECRET", val);
        }
        if let Some(val) = saved_sso {
            std::env::set_var("SSO_ENCRYPTION_KEY", val);
        }
    });
}

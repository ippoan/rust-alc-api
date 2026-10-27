use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde_json::Value;

use crate::mock_helpers::app_state::setup_mock_app_state;
use crate::mock_helpers::MockBotAdminRepository;

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
                bot_id: "8977068".to_string(),
                enabled: true,
                bot_secret_encrypted: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
        ];

        let mut state = setup_mock_app_state();
        state.bot_admin = mock;
        let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

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
        // LINE WORKS Bot の Private Key は export しない (Refs #747)
        assert!(body["data"]["bot_configs"][0]
            .get("private_key_encrypted")
            .is_none());
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
        let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

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
        let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

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
        let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

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
        let base_url = crate::mock_helpers::app_state::spawn_mock_server(state).await;

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

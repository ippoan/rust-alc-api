// SSO Admin — カバレッジ専用テスト (RENAME/trigger/env var)

use super::common;

async fn setup_sso_admin() -> (
    rust_alc_api::AppState,
    String,
    uuid::Uuid,
    String,
    reqwest::Client,
) {
    std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant_id = common::create_test_tenant(
        &state.pool,
        &format!("SSO{}", uuid::Uuid::new_v4().simple()),
    )
    .await;
    let (user_id, _) =
        common::create_test_user_in_db(&state.pool, tenant_id, "ssoadmin@test.com", "admin").await;
    let jwt = common::create_test_jwt_for_user(user_id, tenant_id, "ssoadmin@test.com", "admin");
    let client = reqwest::Client::new();
    (state, base_url, tenant_id, jwt, client)
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_sso_upsert_empty_client_secret() {
    test_group!("SSO管理");
    test_case!("空のclient_secretでupsert → None分岐", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = common::db_rename_flock();
        let (state, base_url, _tenant_id, jwt, client) = setup_sso_admin().await;

        let res = client
            .post(format!("{base_url}/api/admin/sso/configs"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({
                "provider": "lineworks",
                "client_id": "empty-secret-test",
                "client_secret": "",
                "external_org_id": "empty-org"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
        let _ = state;
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_sso_upsert_encrypt_error() {
    test_group!("SSO管理");
    test_case!("JWT_SECRET未設定 → 暗号化エラー → 500", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = common::db_rename_flock();
        let _env = common::ENV_LOCK.lock().unwrap();
        std::env::remove_var("SSO_ENCRYPTION_KEY");
        std::env::remove_var("JWT_SECRET");

        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(
            &state.pool,
            &format!("SSOEnc{}", uuid::Uuid::new_v4().simple()),
        )
        .await;
        let (user_id, _) =
            common::create_test_user_in_db(&state.pool, tenant_id, "ssoenc@test.com", "admin")
                .await;
        let jwt = common::create_test_jwt_for_user(user_id, tenant_id, "ssoenc@test.com", "admin");
        let client = reqwest::Client::new();

        let res = client
            .post(format!("{base_url}/api/admin/sso/configs"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({
                "provider": "lineworks",
                "client_id": "enc-err-test",
                "client_secret": "some-secret",
                "external_org_id": "enc-err-org"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        std::env::set_var("JWT_SECRET", common::TEST_JWT_SECRET);
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_sso_list_configs_db_error() {
    test_group!("SSO管理 DB エラー");
    test_case!("list_configs: RENAME → 500", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = common::db_rename_flock();
        let (state, base_url, _tenant_id, jwt, client) = setup_sso_admin().await;

        sqlx::query("ALTER TABLE alc_api.sso_provider_configs RENAME TO sso_provider_configs_bak")
            .execute(&state.pool)
            .await
            .unwrap();

        let res = client
            .get(format!("{base_url}/api/admin/sso/configs"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("ALTER TABLE alc_api.sso_provider_configs_bak RENAME TO sso_provider_configs")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_sso_upsert_db_error() {
    test_group!("SSO管理 DB エラー");
    test_case!("upsert: trigger → 500", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = common::db_rename_flock();
        let (state, base_url, _tenant_id, jwt, client) = setup_sso_admin().await;

        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.fail_sso_insert() RETURNS trigger AS $$
            BEGIN RAISE EXCEPTION 'test: sso insert blocked'; END;
            $$ LANGUAGE plpgsql"#,
        )
        .execute(&state.pool)
        .await
        .unwrap();
        sqlx::query("CREATE OR REPLACE TRIGGER fail_sso_insert BEFORE INSERT ON alc_api.sso_provider_configs FOR EACH ROW EXECUTE FUNCTION alc_api.fail_sso_insert()")
            .execute(&state.pool).await.unwrap();

        let res = client
            .post(format!("{base_url}/api/admin/sso/configs"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({
                "provider": "lineworks", "client_id": "trigger-test",
                "client_secret": "trigger-secret", "external_org_id": "trigger-org"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("DROP TRIGGER fail_sso_insert ON alc_api.sso_provider_configs")
            .execute(&state.pool)
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.fail_sso_insert")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_sso_delete_db_error() {
    test_group!("SSO管理 DB エラー");
    test_case!("delete: RENAME → 500", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = common::db_rename_flock();
        let (state, base_url, _tenant_id, jwt, client) = setup_sso_admin().await;

        sqlx::query("ALTER TABLE alc_api.sso_provider_configs RENAME TO sso_provider_configs_bak")
            .execute(&state.pool)
            .await
            .unwrap();

        let res = client
            .delete(format!("{base_url}/api/admin/sso/configs"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({ "provider": "lineworks" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("ALTER TABLE alc_api.sso_provider_configs_bak RENAME TO sso_provider_configs")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

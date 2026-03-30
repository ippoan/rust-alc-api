use crate::common;

// ============================================================
// Tenant Users + Bot Admin DB error injection tests
// ============================================================

/// Helper: setup admin user with JWT containing user_id claim
async fn setup_admin_for_coverage() -> (
    rust_alc_api::AppState,
    String,
    uuid::Uuid,
    String,
    reqwest::Client,
) {
    std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    let state = crate::common::setup_app_state().await;
    let base_url = crate::common::spawn_test_server(state.clone()).await;
    let tenant_id = crate::common::create_test_tenant(
        state.pool(),
        &format!("AdmCov{}", uuid::Uuid::new_v4().simple()),
    )
    .await;
    let (user_id, _) =
        crate::common::create_test_user_in_db(state.pool(), tenant_id, "admcov@test.com", "admin")
            .await;
    let jwt =
        crate::common::create_test_jwt_for_user(user_id, tenant_id, "admcov@test.com", "admin");
    let client = reqwest::Client::new();
    (state, base_url, tenant_id, jwt, client)
}

// ============================================================
// tenant_users: list_users DB error (RENAME users)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenant_users_list_users_db_error() {
    test_group!("tenant_users カバレッジ");
    test_case!("list_users: RENAME users → 500", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let (state, base_url, _tenant_id, jwt, client) = setup_admin_for_coverage().await;

        sqlx::query("ALTER TABLE alc_api.users RENAME TO users_bak_tu")
            .execute(state.pool())
            .await
            .unwrap();

        let res = client
            .get(format!("{base_url}/api/admin/users"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("ALTER TABLE alc_api.users_bak_tu RENAME TO users")
            .execute(state.pool())
            .await
            .unwrap();
    });
}

// ============================================================
// tenant_users: list_invitations DB error (RENAME tenant_allowed_emails)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenant_users_list_invitations_db_error() {
    test_group!("tenant_users カバレッジ");
    test_case!("list_invitations: RENAME tenant_allowed_emails → 500", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let (state, base_url, _tenant_id, jwt, client) = setup_admin_for_coverage().await;

        sqlx::query(
            "ALTER TABLE alc_api.tenant_allowed_emails RENAME TO tenant_allowed_emails_bak_tu",
        )
        .execute(state.pool())
        .await
        .unwrap();

        let res = client
            .get(format!("{base_url}/api/admin/users/invitations"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query(
            "ALTER TABLE alc_api.tenant_allowed_emails_bak_tu RENAME TO tenant_allowed_emails",
        )
        .execute(state.pool())
        .await
        .unwrap();
    });
}

// ============================================================
// tenant_users: invite_user DB error (RENAME tenant_allowed_emails)
// invite_user uses state.pool directly (no conn), so RENAME works
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenant_users_invite_user_db_error() {
    test_group!("tenant_users カバレッジ");
    test_case!("invite_user: RENAME tenant_allowed_emails → 500", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let (state, base_url, _tenant_id, jwt, client) = setup_admin_for_coverage().await;

        sqlx::query(
            "ALTER TABLE alc_api.tenant_allowed_emails RENAME TO tenant_allowed_emails_bak_inv",
        )
        .execute(state.pool())
        .await
        .unwrap();

        let res = client
            .post(format!("{base_url}/api/admin/users/invite"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({
                "email": "invite-err@test.com",
                "role": "admin"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query(
            "ALTER TABLE alc_api.tenant_allowed_emails_bak_inv RENAME TO tenant_allowed_emails",
        )
        .execute(state.pool())
        .await
        .unwrap();
    });
}

// ============================================================
// tenant_users: delete_invitation DB error (RENAME tenant_allowed_emails)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenant_users_delete_invitation_db_error() {
    test_group!("tenant_users カバレッジ");
    test_case!("delete_invitation: RENAME tenant_allowed_emails → 500", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let (state, base_url, _tenant_id, jwt, client) = setup_admin_for_coverage().await;

        sqlx::query(
            "ALTER TABLE alc_api.tenant_allowed_emails RENAME TO tenant_allowed_emails_bak_di",
        )
        .execute(state.pool())
        .await
        .unwrap();

        let fake_id = uuid::Uuid::new_v4();
        let res = client
            .delete(format!("{base_url}/api/admin/users/invite/{fake_id}"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query(
            "ALTER TABLE alc_api.tenant_allowed_emails_bak_di RENAME TO tenant_allowed_emails",
        )
        .execute(state.pool())
        .await
        .unwrap();
    });
}

// ============================================================
// tenant_users: delete_user DB error (RENAME users)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenant_users_delete_user_db_error() {
    test_group!("tenant_users カバレッジ");
    test_case!("delete_user: RENAME users → 500", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let (state, base_url, _tenant_id, jwt, client) = setup_admin_for_coverage().await;

        sqlx::query("ALTER TABLE alc_api.users RENAME TO users_bak_du")
            .execute(state.pool())
            .await
            .unwrap();

        let fake_id = uuid::Uuid::new_v4();
        let res = client
            .delete(format!("{base_url}/api/admin/users/{fake_id}"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("ALTER TABLE alc_api.users_bak_du RENAME TO users")
            .execute(state.pool())
            .await
            .unwrap();
    });
}

// ============================================================
// bot_admin: list_configs DB error (RENAME bot_configs)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_bot_list_configs_db_error() {
    test_group!("bot_admin カバレッジ");
    test_case!("list_configs: RENAME bot_configs → 500", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let (state, base_url, _tenant_id, jwt, client) = setup_admin_for_coverage().await;

        sqlx::query("ALTER TABLE alc_api.bot_configs RENAME TO bot_configs_bak_lc")
            .execute(state.pool())
            .await
            .unwrap();

        let res = client
            .get(format!("{base_url}/api/admin/bot/configs"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("ALTER TABLE alc_api.bot_configs_bak_lc RENAME TO bot_configs")
            .execute(state.pool())
            .await
            .unwrap();
    });
}

// ============================================================
// bot_admin: upsert_config DB error (trigger on bot_configs INSERT)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_bot_upsert_db_error() {
    test_group!("bot_admin カバレッジ");
    test_case!("upsert_config: INSERT trigger → 500", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let (state, base_url, _tenant_id, jwt, client) = setup_admin_for_coverage().await;

        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.reject_bot_config_insert() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: bot_configs insert blocked'; END;
               $$ LANGUAGE plpgsql"#,
        )
        .execute(state.pool())
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER reject_bot_config_insert BEFORE INSERT ON alc_api.bot_configs \
             FOR EACH ROW EXECUTE FUNCTION alc_api.reject_bot_config_insert()",
        )
        .execute(state.pool())
        .await
        .unwrap();

        let res = client
            .post(format!("{base_url}/api/admin/bot/configs"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({
                "name": "err-bot",
                "client_id": "cid",
                "service_account": "sa",
                "bot_id": "bid"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("DROP TRIGGER reject_bot_config_insert ON alc_api.bot_configs")
            .execute(state.pool())
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.reject_bot_config_insert()")
            .execute(state.pool())
            .await
            .unwrap();
    });
}

// ============================================================
// bot_admin: delete_config DB error (RENAME bot_configs)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_bot_delete_db_error() {
    test_group!("bot_admin カバレッジ");
    test_case!("delete_config: RENAME bot_configs → 500", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let (state, base_url, _tenant_id, jwt, client) = setup_admin_for_coverage().await;

        sqlx::query("ALTER TABLE alc_api.bot_configs RENAME TO bot_configs_bak_dc")
            .execute(state.pool())
            .await
            .unwrap();

        let fake_id = uuid::Uuid::new_v4();
        let res = client
            .delete(format!("{base_url}/api/admin/bot/configs"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({ "id": fake_id.to_string() }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("ALTER TABLE alc_api.bot_configs_bak_dc RENAME TO bot_configs")
            .execute(state.pool())
            .await
            .unwrap();
    });
}

// ============================================================
// bot_admin: upsert_config encrypt error (no JWT_SECRET env var)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_bot_upsert_encrypt_error_no_key() {
    test_group!("bot_admin カバレッジ");
    test_case!("upsert_config: no encryption key → 500", {
        let _env = crate::common::ENV_LOCK.lock().unwrap();
        let (_state, base_url, _tenant_id, jwt, client) = setup_admin_for_coverage().await;

        // Remove encryption keys
        std::env::remove_var("SSO_ENCRYPTION_KEY");
        std::env::remove_var("JWT_SECRET");

        let res = client
            .post(format!("{base_url}/api/admin/bot/configs"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({
                "name": "no-key-bot",
                "client_id": "cid",
                "service_account": "sa",
                "bot_id": "bid"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        // Restore
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
    });
}

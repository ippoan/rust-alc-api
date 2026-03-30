use crate::common;

// ============================================================
// tenko_call register — tx.begin() エラー (pool.close)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_call_register_pool_closed() {
    test_group!("tenko_call カバレッジ");
    test_case!("register: pool closed → 500 + register_err カバー", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        state.pool().close().await;

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko-call/register"))
            .json(&serde_json::json!({
                "phone_number": "090-pool-err",
                "driver_name": "プールエラー",
                "call_number": "nonexistent"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

// ============================================================
// tenko_call register — trigger で driver upsert 拒否
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_call_register_trigger_error() {
    test_group!("tenko_call カバレッジ");
    test_case!("register: driver upsert trigger 失敗 → 500", {
        let _lock = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(state.pool(), "TkRegTrig").await;

        let call_num = format!("err-reg-{}", uuid::Uuid::new_v4().simple());
        sqlx::query("INSERT INTO tenko_call_numbers (call_number, tenant_id) VALUES ($1, $2)")
            .bind(&call_num)
            .bind(tenant_id.to_string())
            .execute(state.pool())
            .await
            .unwrap();

        // trigger: tenko_call_drivers INSERT/UPDATE を拒否
        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.reject_tenko_driver_fn() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: blocked'; END;
               $$ LANGUAGE plpgsql"#,
        )
        .execute(state.pool())
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER reject_tenko_driver BEFORE INSERT OR UPDATE ON alc_api.tenko_call_drivers FOR EACH ROW EXECUTE FUNCTION alc_api.reject_tenko_driver_fn()",
        )
        .execute(state.pool())
        .await
        .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko-call/register"))
            .json(&serde_json::json!({
                "phone_number": "090-trig-err",
                "driver_name": "トリガーエラー",
                "call_number": call_num
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        // Cleanup
        sqlx::query("DROP TRIGGER reject_tenko_driver ON alc_api.tenko_call_drivers")
            .execute(state.pool())
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.reject_tenko_driver_fn()")
            .execute(state.pool())
            .await
            .unwrap();
    });
}

// ============================================================
// tenko_call tenko — tx.begin() エラー (pool.close)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_call_tenko_pool_closed() {
    test_group!("tenko_call カバレッジ");
    test_case!("tenko: pool closed → 500", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        state.pool().close().await;

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko-call/tenko"))
            .json(&serde_json::json!({
                "phone_number": "090-pool-tenko",
                "driver_name": "プールエラー",
                "latitude": 35.0,
                "longitude": 139.0
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

// ============================================================
// tenko_call tenko — trigger で log insert 拒否
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_call_tenko_trigger_error() {
    test_group!("tenko_call カバレッジ");
    test_case!("tenko: log insert trigger 失敗 → 500", {
        let _lock = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(state.pool(), "TkTenkoTrig").await;

        let call_num = format!("err-tk-{}", uuid::Uuid::new_v4().simple());
        let phone = format!("090-tk-{}", uuid::Uuid::new_v4().simple());

        sqlx::query("INSERT INTO tenko_call_numbers (call_number, tenant_id) VALUES ($1, $2)")
            .bind(&call_num)
            .bind(tenant_id.to_string())
            .execute(state.pool())
            .await
            .unwrap();

        sqlx::query("SELECT set_current_tenant($1)")
            .bind(tenant_id.to_string())
            .execute(state.pool())
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO tenko_call_drivers (phone_number, driver_name, call_number, tenant_id) VALUES ($1, $2, $3, $4)",
        )
        .bind(&phone)
        .bind("テンコテスト")
        .bind(&call_num)
        .bind(tenant_id.to_string())
        .execute(state.pool())
        .await
        .unwrap();

        // trigger: tenko_call_logs INSERT を拒否
        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.reject_tenko_log_fn() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: blocked'; END;
               $$ LANGUAGE plpgsql"#,
        )
        .execute(state.pool())
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER reject_tenko_log BEFORE INSERT ON alc_api.tenko_call_logs FOR EACH ROW EXECUTE FUNCTION alc_api.reject_tenko_log_fn()",
        )
        .execute(state.pool())
        .await
        .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko-call/tenko"))
            .json(&serde_json::json!({
                "phone_number": phone,
                "driver_name": "テンコテスト",
                "latitude": 35.0,
                "longitude": 139.0
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        // Cleanup
        sqlx::query("DROP TRIGGER reject_tenko_log ON alc_api.tenko_call_logs")
            .execute(state.pool())
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.reject_tenko_log_fn()")
            .execute(state.pool())
            .await
            .unwrap();
    });
}

// ============================================================
// register — master lookup エラー (RENAME tenko_call_numbers)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_call_register_master_lookup_rename() {
    test_group!("tenko_call カバレッジ");
    test_case!("register: master lookup RENAME → 500", {
        let _lock = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;

        sqlx::query("ALTER TABLE alc_api.tenko_call_numbers RENAME TO tenko_call_numbers_bak3")
            .execute(state.pool())
            .await
            .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko-call/register"))
            .json(&serde_json::json!({
                "phone_number": "090-rename-err",
                "driver_name": "RENAMEエラー",
                "call_number": "nonexistent"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("ALTER TABLE alc_api.tenko_call_numbers_bak3 RENAME TO tenko_call_numbers")
            .execute(state.pool())
            .await
            .unwrap();
    });
}

// ============================================================
// register — set_tenant エラー (DROP set_current_tenant)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_call_register_set_tenant_error() {
    test_group!("tenko_call カバレッジ");
    test_case!("register: set_tenant DROP → 500", {
        let _lock = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(state.pool(), "TkSetTenErr").await;

        let call_num = format!("err-st-{}", uuid::Uuid::new_v4().simple());
        sqlx::query("INSERT INTO tenko_call_numbers (call_number, tenant_id) VALUES ($1, $2)")
            .bind(&call_num)
            .bind(tenant_id.to_string())
            .execute(state.pool())
            .await
            .unwrap();

        // set_current_tenant を RENAME (DROP は CASCADE で危険)
        sqlx::query(
            "ALTER FUNCTION alc_api.set_current_tenant(text) RENAME TO set_current_tenant_bak",
        )
        .execute(state.pool())
        .await
        .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko-call/register"))
            .json(&serde_json::json!({
                "phone_number": "090-st-err",
                "driver_name": "SetTenantエラー",
                "call_number": call_num
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query(
            "ALTER FUNCTION alc_api.set_current_tenant_bak(text) RENAME TO set_current_tenant",
        )
        .execute(state.pool())
        .await
        .unwrap();
    });
}

// ============================================================
// tenko — driver lookup エラー (RENAME tenko_call_drivers)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_call_tenko_driver_lookup_rename() {
    test_group!("tenko_call カバレッジ");
    test_case!("tenko: driver lookup RENAME → 500", {
        let _lock = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;

        sqlx::query("ALTER TABLE alc_api.tenko_call_drivers RENAME TO tenko_call_drivers_bak3")
            .execute(state.pool())
            .await
            .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko-call/tenko"))
            .json(&serde_json::json!({
                "phone_number": "090-rename-drv",
                "driver_name": "RENAMEドライバー",
                "latitude": 35.0,
                "longitude": 139.0
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("ALTER TABLE alc_api.tenko_call_drivers_bak3 RENAME TO tenko_call_drivers")
            .execute(state.pool())
            .await
            .unwrap();
    });
}

// ============================================================
// tenko — set_tenant エラー (RENAME set_current_tenant)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_call_tenko_set_tenant_error() {
    test_group!("tenko_call カバレッジ");
    test_case!("tenko: set_tenant RENAME → 500", {
        let _lock = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(state.pool(), "TkTkSetTenErr").await;

        let call_num = format!("err-tkst-{}", uuid::Uuid::new_v4().simple());
        let phone = format!("090-tkst-{}", uuid::Uuid::new_v4().simple());

        sqlx::query("INSERT INTO tenko_call_numbers (call_number, tenant_id) VALUES ($1, $2)")
            .bind(&call_num)
            .bind(tenant_id.to_string())
            .execute(state.pool())
            .await
            .unwrap();

        sqlx::query("SELECT set_current_tenant($1)")
            .bind(tenant_id.to_string())
            .execute(state.pool())
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO tenko_call_drivers (phone_number, driver_name, call_number, tenant_id) VALUES ($1, $2, $3, $4)",
        )
        .bind(&phone)
        .bind("SetTenantテスト")
        .bind(&call_num)
        .bind(tenant_id.to_string())
        .execute(state.pool())
        .await
        .unwrap();

        // set_current_tenant を RENAME
        sqlx::query(
            "ALTER FUNCTION alc_api.set_current_tenant(text) RENAME TO set_current_tenant_bak",
        )
        .execute(state.pool())
        .await
        .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko-call/tenko"))
            .json(&serde_json::json!({
                "phone_number": phone,
                "driver_name": "SetTenantテスト",
                "latitude": 35.0,
                "longitude": 139.0
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query(
            "ALTER FUNCTION alc_api.set_current_tenant_bak(text) RENAME TO set_current_tenant",
        )
        .execute(state.pool())
        .await
        .unwrap();
    });
}

// ============================================================
// register — tx.commit() エラー (deferred constraint trigger)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_call_register_commit_error() {
    test_group!("tenko_call カバレッジ");
    test_case!("register: deferred trigger → commit 失敗 → 500", {
        let _lock = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(state.pool(), "TkCommitErr").await;

        let call_num = format!("err-cm-{}", uuid::Uuid::new_v4().simple());
        sqlx::query("INSERT INTO tenko_call_numbers (call_number, tenant_id) VALUES ($1, $2)")
            .bind(&call_num)
            .bind(tenant_id.to_string())
            .execute(state.pool())
            .await
            .unwrap();

        // CONSTRAINT trigger (DEFERRABLE INITIALLY DEFERRED) → commit 時に RAISE
        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.reject_commit_driver_fn() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: commit blocked'; END;
               $$ LANGUAGE plpgsql"#,
        )
        .execute(state.pool())
        .await
        .unwrap();
        sqlx::query(
            "CREATE CONSTRAINT TRIGGER reject_commit_driver AFTER INSERT OR UPDATE ON alc_api.tenko_call_drivers DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION alc_api.reject_commit_driver_fn()",
        )
        .execute(state.pool())
        .await
        .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko-call/register"))
            .json(&serde_json::json!({
                "phone_number": format!("090-cm-{}", uuid::Uuid::new_v4().simple()),
                "driver_name": "コミットエラー",
                "call_number": call_num
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        // Cleanup
        sqlx::query("DROP TRIGGER reject_commit_driver ON alc_api.tenko_call_drivers")
            .execute(state.pool())
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.reject_commit_driver_fn()")
            .execute(state.pool())
            .await
            .unwrap();
    });
}

// ============================================================
// tenko — tx.commit() エラー (deferred constraint trigger)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_call_tenko_commit_error() {
    test_group!("tenko_call カバレッジ");
    test_case!("tenko: deferred trigger → commit 失敗 → 500", {
        let _lock = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(state.pool(), "TkTkCommitErr").await;

        let call_num = format!("err-tkcm-{}", uuid::Uuid::new_v4().simple());
        let phone = format!("090-tkcm-{}", uuid::Uuid::new_v4().simple());

        sqlx::query("INSERT INTO tenko_call_numbers (call_number, tenant_id) VALUES ($1, $2)")
            .bind(&call_num)
            .bind(tenant_id.to_string())
            .execute(state.pool())
            .await
            .unwrap();

        sqlx::query("SELECT set_current_tenant($1)")
            .bind(tenant_id.to_string())
            .execute(state.pool())
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO tenko_call_drivers (phone_number, driver_name, call_number, tenant_id) VALUES ($1, $2, $3, $4)",
        )
        .bind(&phone)
        .bind("コミットテスト")
        .bind(&call_num)
        .bind(tenant_id.to_string())
        .execute(state.pool())
        .await
        .unwrap();

        // CONSTRAINT trigger (DEFERRABLE INITIALLY DEFERRED) on tenko_call_logs → commit 時に RAISE
        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.reject_commit_log_fn() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: commit blocked'; END;
               $$ LANGUAGE plpgsql"#,
        )
        .execute(state.pool())
        .await
        .unwrap();
        sqlx::query(
            "CREATE CONSTRAINT TRIGGER reject_commit_log AFTER INSERT ON alc_api.tenko_call_logs DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION alc_api.reject_commit_log_fn()",
        )
        .execute(state.pool())
        .await
        .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko-call/tenko"))
            .json(&serde_json::json!({
                "phone_number": phone,
                "driver_name": "コミットテスト",
                "latitude": 35.0,
                "longitude": 139.0
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        // Cleanup
        sqlx::query("DROP TRIGGER reject_commit_log ON alc_api.tenko_call_logs")
            .execute(state.pool())
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.reject_commit_log_fn()")
            .execute(state.pool())
            .await
            .unwrap();
    });
}

// ============================================================
// list_numbers / create_number / delete_number / list_drivers — DB エラー (pool.close)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_call_list_numbers_db_error() {
    test_group!("tenko_call カバレッジ");
    test_case!("list_numbers: pool closed → 500", {
        let state = common::setup_app_state().await;
        let tenant_id = common::create_test_tenant(state.pool(), "TkListNumErr").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let base_url = common::spawn_test_server(state.clone()).await;
        state.pool().close().await;

        let client = reqwest::Client::new();
        let res = client
            .get(format!("{base_url}/api/tenko-call/numbers"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_call_create_number_db_error() {
    test_group!("tenko_call カバレッジ");
    test_case!("create_number: pool closed → 500", {
        let state = common::setup_app_state().await;
        let tenant_id = common::create_test_tenant(state.pool(), "TkCreateNumErr").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let base_url = common::spawn_test_server(state.clone()).await;
        state.pool().close().await;

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko-call/numbers"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({
                "call_number": "err-create-001",
                "label": "test"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_call_delete_number_db_error() {
    test_group!("tenko_call カバレッジ");
    test_case!("delete_number: pool closed → 500", {
        let state = common::setup_app_state().await;
        let tenant_id = common::create_test_tenant(state.pool(), "TkDelNumErr").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let base_url = common::spawn_test_server(state.clone()).await;
        state.pool().close().await;

        let client = reqwest::Client::new();
        let res = client
            .delete(format!("{base_url}/api/tenko-call/numbers/99999"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_call_list_drivers_db_error() {
    test_group!("tenko_call カバレッジ");
    test_case!("list_drivers: pool closed → 500", {
        let state = common::setup_app_state().await;
        let tenant_id = common::create_test_tenant(state.pool(), "TkListDrvErr").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let base_url = common::spawn_test_server(state.clone()).await;
        state.pool().close().await;

        let client = reqwest::Client::new();
        let res = client
            .get(format!("{base_url}/api/tenko-call/drivers"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);
    });
}

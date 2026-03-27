use crate::common;

// ============================================================
// tenko_call register — DB エラー (trigger で INSERT 拒否)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_call_register_db_error() {
    test_group!("tenko_call カバレッジ");
    test_case!(
        "register: driver upsert 失敗 → 500 + register_err カバー",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_id = common::create_test_tenant(&state.pool, "TkCallErr").await;

            let call_num = format!("err-call-{}", uuid::Uuid::new_v4().simple());

            // マスタ登録 (register の call_number 検証を通すため)
            sqlx::query("INSERT INTO tenko_call_numbers (call_number, tenant_id) VALUES ($1, $2)")
                .bind(&call_num)
                .bind(tenant_id.to_string())
                .execute(&state.pool)
                .await
                .unwrap();

            // trigger: tenko_call_drivers への INSERT/UPDATE を拒否
            sqlx::query(
                r#"CREATE OR REPLACE FUNCTION alc_api.reject_tenko_driver_fn() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: tenko driver insert blocked'; END;
               $$ LANGUAGE plpgsql"#,
            )
            .execute(&state.pool)
            .await
            .unwrap();
            sqlx::query(
            "CREATE TRIGGER reject_tenko_driver BEFORE INSERT OR UPDATE ON alc_api.tenko_call_drivers FOR EACH ROW EXECUTE FUNCTION alc_api.reject_tenko_driver_fn()",
        )
        .execute(&state.pool)
        .await
        .unwrap();

            let client = reqwest::Client::new();
            let res = client
                .post(format!("{base_url}/api/tenko-call/register"))
                .json(&serde_json::json!({
                    "phone_number": "090-0000-err1",
                    "driver_name": "エラーテスト",
                    "call_number": call_num
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 500);
            let body: serde_json::Value = res.json().await.unwrap();
            assert_eq!(body["success"], false);
            assert!(body["error"].as_str().unwrap().contains("internal error"));

            // Cleanup
            sqlx::query("DROP TRIGGER reject_tenko_driver ON alc_api.tenko_call_drivers")
                .execute(&state.pool)
                .await
                .unwrap();
            sqlx::query("DROP FUNCTION alc_api.reject_tenko_driver_fn()")
                .execute(&state.pool)
                .await
                .unwrap();
        }
    );
}

// ============================================================
// tenko_call tenko — DB エラー (trigger で tenko_call_logs INSERT 拒否)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_call_tenko_db_error() {
    test_group!("tenko_call カバレッジ");
    test_case!("tenko: log insert 失敗 → 500", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "TkTenkoErr").await;

        let call_num = format!("err-tenko-{}", uuid::Uuid::new_v4().simple());
        let phone = format!("090-err-{}", uuid::Uuid::new_v4().simple());

        // マスタ登録
        sqlx::query("INSERT INTO tenko_call_numbers (call_number, tenant_id) VALUES ($1, $2)")
            .bind(&call_num)
            .bind(tenant_id.to_string())
            .execute(&state.pool)
            .await
            .unwrap();

        // set_current_tenant してからドライバー INSERT
        sqlx::query("SELECT set_current_tenant($1)")
            .bind(tenant_id.to_string())
            .execute(&state.pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO tenko_call_drivers (phone_number, driver_name, call_number, tenant_id) VALUES ($1, $2, $3, $4)",
        )
        .bind(&phone)
        .bind("テンコエラー")
        .bind(&call_num)
        .bind(tenant_id.to_string())
        .execute(&state.pool)
        .await
        .unwrap();

        // trigger: tenko_call_logs への INSERT を拒否
        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.reject_tenko_log_fn() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: tenko log insert blocked'; END;
               $$ LANGUAGE plpgsql"#,
        )
        .execute(&state.pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER reject_tenko_log BEFORE INSERT ON alc_api.tenko_call_logs FOR EACH ROW EXECUTE FUNCTION alc_api.reject_tenko_log_fn()",
        )
        .execute(&state.pool)
        .await
        .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko-call/tenko"))
            .json(&serde_json::json!({
                "phone_number": phone,
                "driver_name": "テンコエラー",
                "latitude": 35.0,
                "longitude": 139.0
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        // Cleanup
        sqlx::query("DROP TRIGGER reject_tenko_log ON alc_api.tenko_call_logs")
            .execute(&state.pool)
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.reject_tenko_log_fn()")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

// ============================================================
// register — master lookup DB エラー (RENAME tenko_call_numbers)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_call_register_master_lookup_error() {
    test_group!("tenko_call カバレッジ");
    test_case!("register: master lookup 失敗 → 500", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;

        // RENAME で master lookup を壊す
        sqlx::query("ALTER TABLE alc_api.tenko_call_numbers RENAME TO tenko_call_numbers_bak2")
            .execute(&state.pool)
            .await
            .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko-call/register"))
            .json(&serde_json::json!({
                "phone_number": "090-master-err",
                "driver_name": "マスタエラー",
                "call_number": "nonexistent"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("ALTER TABLE alc_api.tenko_call_numbers_bak2 RENAME TO tenko_call_numbers")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

// ============================================================
// tenko — driver lookup DB エラー (RENAME tenko_call_drivers)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_call_tenko_driver_lookup_error() {
    test_group!("tenko_call カバレッジ");
    test_case!("tenko: driver lookup 失敗 → 500", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;

        // RENAME で driver lookup を壊す
        sqlx::query("ALTER TABLE alc_api.tenko_call_drivers RENAME TO tenko_call_drivers_bak2")
            .execute(&state.pool)
            .await
            .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko-call/tenko"))
            .json(&serde_json::json!({
                "phone_number": "090-drv-err",
                "driver_name": "ドライバーエラー",
                "latitude": 35.0,
                "longitude": 139.0
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("ALTER TABLE alc_api.tenko_call_drivers_bak2 RENAME TO tenko_call_drivers")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

// ============================================================
// list_numbers / create_number / list_drivers — DB エラー (RENAME)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_call_list_numbers_db_error() {
    test_group!("tenko_call カバレッジ");
    test_case!("list_numbers: テーブル RENAME → 500", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "TkListNumErr").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");

        sqlx::query("ALTER TABLE alc_api.tenko_call_numbers RENAME TO tenko_call_numbers_bak")
            .execute(&state.pool)
            .await
            .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .get(format!("{base_url}/api/tenko-call/numbers"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("ALTER TABLE alc_api.tenko_call_numbers_bak RENAME TO tenko_call_numbers")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_call_create_number_db_error() {
    test_group!("tenko_call カバレッジ");
    test_case!("create_number: テーブル RENAME → 500", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "TkCreateNumErr").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");

        sqlx::query("ALTER TABLE alc_api.tenko_call_numbers RENAME TO tenko_call_numbers_bak")
            .execute(&state.pool)
            .await
            .unwrap();

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

        sqlx::query("ALTER TABLE alc_api.tenko_call_numbers_bak RENAME TO tenko_call_numbers")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_call_delete_number_db_error() {
    test_group!("tenko_call カバレッジ");
    test_case!("delete_number: テーブル RENAME → 500", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "TkDelNumErr").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");

        sqlx::query("ALTER TABLE alc_api.tenko_call_numbers RENAME TO tenko_call_numbers_bak")
            .execute(&state.pool)
            .await
            .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .delete(format!("{base_url}/api/tenko-call/numbers/99999"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("ALTER TABLE alc_api.tenko_call_numbers_bak RENAME TO tenko_call_numbers")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_call_list_drivers_db_error() {
    test_group!("tenko_call カバレッジ");
    test_case!("list_drivers: テーブル RENAME → 500", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(&state.pool, "TkListDrvErr").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");

        sqlx::query("ALTER TABLE alc_api.tenko_call_drivers RENAME TO tenko_call_drivers_bak")
            .execute(&state.pool)
            .await
            .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .get(format!("{base_url}/api/tenko-call/drivers"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("ALTER TABLE alc_api.tenko_call_drivers_bak RENAME TO tenko_call_drivers")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

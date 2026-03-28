/// dtako_operations month==12, middleware auth fallback, nfc_tags DBエラー, health_baselines DBエラー
/// car_inspection get_by_id 成功, employees update_employee ISE, communication_items list DBエラー, sso_admin client_secret None

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_calendar_december() {
    test_group!("カバレッジ 100% 補完");
    test_case!("12月のカレンダーで翌年1月1日を計算する", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "CalDec").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        let client = reqwest::Client::new();
        let res = client
            .get(format!(
                "{base_url}/api/operations/calendar?year=2026&month=12"
            ))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: serde_json::Value = res.json().await.unwrap();
        assert_eq!(body["year"], 2026);
        assert_eq!(body["month"], 12);
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_auth_jwt_fail_fallback_to_tenant_id() {
    test_group!("カバレッジ 100% 補完");
    test_case!(
        "不正 JWT + 有効な X-Tenant-ID でフォールバック成功する",
        {
            let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
            let _flock = crate::common::db_rename_flock();
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(&state.pool, "AuthFB").await;

            let client = reqwest::Client::new();
            let res = client
                .get(format!("{base_url}/api/employees"))
                .header("Authorization", "Bearer invalid-jwt-token")
                .header("X-Tenant-ID", tenant_id.to_string())
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
        }
    );
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_nfc_tag_register_db_error() {
    test_group!("カバレッジ 100% 補完");
    test_case!("NFC タグ登録で DB エラー時に 500 を返す", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "NFCErr").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.reject_nfc_insert() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: nfc insert blocked'; END;
               $$ LANGUAGE plpgsql"#,
        )
        .execute(&state.pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER reject_nfc_insert BEFORE INSERT ON alc_api.car_inspection_nfc_tags \
             FOR EACH ROW EXECUTE FUNCTION alc_api.reject_nfc_insert()",
        )
        .execute(&state.pool)
        .await
        .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/nfc-tags"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({
                "nfc_uuid": "test-nfc-uuid-cov",
                "car_inspection_id": 99999
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("DROP TRIGGER reject_nfc_insert ON alc_api.car_inspection_nfc_tags")
            .execute(&state.pool)
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.reject_nfc_insert()")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_health_baseline_upsert_db_error() {
    test_group!("カバレッジ 100% 補完");
    test_case!(
        "健康基準値 upsert で DB エラー時に 500 を返す",
        {
            let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
            let _flock = crate::common::db_rename_flock();
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(&state.pool, "HBErr").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");

            sqlx::query(
                r#"CREATE OR REPLACE FUNCTION alc_api.reject_hb_insert() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: health baseline insert blocked'; END;
               $$ LANGUAGE plpgsql"#,
            )
            .execute(&state.pool)
            .await
            .unwrap();
            sqlx::query(
                "CREATE TRIGGER reject_hb_insert BEFORE INSERT ON alc_api.employee_health_baselines \
                 FOR EACH ROW EXECUTE FUNCTION alc_api.reject_hb_insert()",
            )
            .execute(&state.pool)
            .await
            .unwrap();

            let client = reqwest::Client::new();
            let res = client
                .post(format!("{base_url}/api/tenko/health-baselines"))
                .header("Authorization", format!("Bearer {jwt}"))
                .json(&serde_json::json!({
                    "employee_id": "00000000-0000-0000-0000-000000000099",
                    "baseline_systolic": 120,
                    "baseline_diastolic": 80,
                    "baseline_temperature": 36.5
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 500);

            sqlx::query("DROP TRIGGER reject_hb_insert ON alc_api.employee_health_baselines")
                .execute(&state.pool)
                .await
                .unwrap();
            sqlx::query("DROP FUNCTION alc_api.reject_hb_insert()")
                .execute(&state.pool)
                .await
                .unwrap();
        }
    );
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_carrying_items_empty_tenant() {
    test_group!("カバレッジ 100% 補完");
    test_case!(
        "carrying_items が空のテナントで一覧取得 → 空配列",
        {
            let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
            let _flock = crate::common::db_rename_flock();
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(&state.pool, "CarryEmpty").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");

            let client = reqwest::Client::new();
            let res = client
                .get(format!("{base_url}/api/carrying-items"))
                .header("Authorization", format!("Bearer {jwt}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Vec<serde_json::Value> = res.json().await.unwrap();
            assert!(body.is_empty(), "New tenant should have no carrying items");
        }
    );
}

// ============================================================
// 1. car_inspections get_by_id success path (line 114)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_car_inspection_get_by_id_success() {
    test_group!("カバレッジ 100% 補完");
    test_case!("car_inspection get_by_id 成功パス", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "CarInsGet").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        // INSERT a minimal car_inspection row with all 96 NOT NULL columns
        let row = sqlx::query_scalar::<_, i32>(
            r#"INSERT INTO alc_api.car_inspection (
                tenant_id,
                "CertInfoImportFileVersion", "Acceptoutputno", "FormType", "ElectCertMgNo",
                "CarId", "ElectCertPublishdateE", "ElectCertPublishdateY",
                "ElectCertPublishdateM", "ElectCertPublishdateD",
                "GrantdateE", "GrantdateY", "GrantdateM", "GrantdateD",
                "TranspotationBureauchiefName", "EntryNoCarNo",
                "ReggrantdateE", "ReggrantdateY", "ReggrantdateM", "ReggrantdateD",
                "FirstregistdateE", "FirstregistdateY", "FirstregistdateM",
                "CarName", "CarNameCode", "CarNo", "Model", "EngineModel",
                "OwnernameLowLevelChar", "OwnernameHighLevelChar",
                "OwnerAddressChar", "OwnerAddressNumValue", "OwnerAddressCode",
                "UsernameLowLevelChar", "UsernameHighLevelChar",
                "UserAddressChar", "UserAddressNumValue", "UserAddressCode",
                "UseheadqrterChar", "UseheadqrterNumValue", "UseheadqrterCode",
                "CarKind", "Use", "PrivateBusiness", "CarShape", "CarShapeCode",
                "NoteCap", "Cap", "NoteMaxloadage", "Maxloadage",
                "NoteCarWgt", "CarWgt", "NoteCarTotalWgt", "CarTotalWgt",
                "NoteLength", "Length", "NoteWidth", "Width",
                "NoteHeight", "Height",
                "FfAxWgt", "FrAxWgt", "RfAxWgt", "RrAxWgt",
                "Displacement", "FuelClass", "ModelSpecifyNo", "ClassifyAroundNo",
                "ValidPeriodExpirdateE", "ValidPeriodExpirdateY",
                "ValidPeriodExpirdateM", "ValidPeriodExpirdateD", "NoteInfo",
                "TwodimensionCodeInfoEntryNoCarNo", "TwodimensionCodeInfoCarNo",
                "TwodimensionCodeInfoValidPeriodExpirdate",
                "TwodimensionCodeInfoModel",
                "TwodimensionCodeInfoModelSpecifyNoClassifyAroundNo",
                "TwodimensionCodeInfoCharInfo", "TwodimensionCodeInfoEngineModel",
                "TwodimensionCodeInfoCarNoStampPlace",
                "TwodimensionCodeInfoFirstregistdate",
                "TwodimensionCodeInfoFfAxWgt", "TwodimensionCodeInfoFrAxWgt",
                "TwodimensionCodeInfoRfAxWgt", "TwodimensionCodeInfoRrAxWgt",
                "TwodimensionCodeInfoNoiseReg", "TwodimensionCodeInfoNearNoiseReg",
                "TwodimensionCodeInfoDriveMethod",
                "TwodimensionCodeInfoOpacimeterMeasCar",
                "TwodimensionCodeInfoNoxPmMeasMode",
                "TwodimensionCodeInfoNoxValue", "TwodimensionCodeInfoPmValue",
                "TwodimensionCodeInfoSafeStdDate",
                "TwodimensionCodeInfoFuelClassCode",
                "RegistCarLightCar"
            ) VALUES (
                $1,
                '1','1','1','MGNO1',
                'CID1','1','26','01','01',
                '1','26','01','01',
                'Bureau','ENT1',
                '1','26','01','01',
                '1','26','01',
                'TestCar','TC','1234','MDL','ENG',
                'Owner','Owner',
                'Addr','1','100',
                'User','User',
                'UAddr','1','200',
                'HQ','1','300',
                'Kind','Use','Priv','Shape','SC',
                '','0','','0',
                '','0','','0',
                '','0','','0',
                '','0',
                '0','0','0','0',
                '0','Gas','MSN','CAN',
                '1','28','01','01','',
                'ENT1','1234',
                '280101',
                'MDL',
                'MSCN',
                'CHR','ENG',
                'STP',
                '2601',
                '0','0','0','0',
                '','',
                '',
                '',
                '',
                '','',
                '',
                '',
                '1'
            ) RETURNING id"#,
        )
        .bind(tenant_id)
        .fetch_one(&state.pool)
        .await
        .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .get(format!("{base_url}/api/car-inspections/{row}"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: serde_json::Value = res.json().await.unwrap();
        assert_eq!(body["CarId"], "CID1");

        // Cleanup
        sqlx::query("DELETE FROM alc_api.car_inspection WHERE id = $1")
            .bind(row)
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

// ============================================================
// 2. employees update_employee non-CONFLICT DB error (line 223)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_update_employee_non_conflict_db_error() {
    test_group!("カバレッジ 100% 補完");
    test_case!("update_employee: 非CONFLICT DBエラー → 500", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "EmpUpdISE").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        // Create an employee first
        let emp =
            crate::common::create_test_employee(&client, &base_url, &auth, "UpdISEEmp", "UI01")
                .await;
        let id = emp["id"].as_str().unwrap();

        // Create a trigger that raises a generic (non-constraint) exception on UPDATE
        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.reject_emp_update_ise() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: generic update error'; END;
               $$ LANGUAGE plpgsql"#,
        )
        .execute(&state.pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER reject_emp_update_ise BEFORE UPDATE ON alc_api.employees \
             FOR EACH ROW EXECUTE FUNCTION alc_api.reject_emp_update_ise()",
        )
        .execute(&state.pool)
        .await
        .unwrap();

        let res = client
            .put(format!("{base_url}/api/employees/{id}"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "name": "Updated",
                "code": "UI02"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        // Cleanup
        sqlx::query("DROP TRIGGER reject_emp_update_ise ON alc_api.employees")
            .execute(&state.pool)
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.reject_emp_update_ise()")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

// ============================================================
// 3. communication_items list DB error (lines 106-108)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_communication_items_list_db_error() {
    test_group!("カバレッジ 100% 補完");
    test_case!("communication_items list: RENAME → 500", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "CommListErr").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        // Rename employees table so the JOIN in the second query fails,
        // but the COUNT query (no join) succeeds.
        sqlx::query("ALTER TABLE alc_api.employees RENAME TO employees_bak")
            .execute(&state.pool)
            .await
            .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .get(format!("{base_url}/api/communication-items"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        // Restore
        sqlx::query("ALTER TABLE alc_api.employees_bak RENAME TO employees")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

// ============================================================
// 4. sso_admin upsert client_secret None path (line 113)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_sso_upsert_no_client_secret() {
    test_group!("カバレッジ 100% 補完");
    test_case!("sso upsert: client_secret省略 → None分岐", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(
            &state.pool,
            &format!("SSONoSec{}", uuid::Uuid::new_v4().simple()),
        )
        .await;
        let (user_id, _) = crate::common::create_test_user_in_db(
            &state.pool,
            tenant_id,
            "ssonosec@test.com",
            "admin",
        )
        .await;
        let jwt = crate::common::create_test_jwt_for_user(
            user_id,
            tenant_id,
            "ssonosec@test.com",
            "admin",
        );
        let client = reqwest::Client::new();

        // Send upsert request WITHOUT client_secret field at all.
        // This hits line 113 (the `None` branch when client_secret is absent).
        // The DB INSERT will fail because client_secret_encrypted is NOT NULL,
        // but the None path (line 113) is covered before the DB call.
        let res = client
            .post(format!("{base_url}/api/admin/sso/configs"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({
                "provider": "lineworks",
                "client_id": "no-secret-test",
                "external_org_id": "no-secret-org"
            }))
            .send()
            .await
            .unwrap();
        // 500 because DB column client_secret_encrypted is NOT NULL
        assert_eq!(res.status(), 500);
    });
}

// ============================================================
// 5. bot_admin: UPDATE with non-empty client_secret & private_key (lines 134, 148)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_bot_admin_update_with_secrets() {
    test_group!("カバレッジ 100% 補完");
    test_case!(
        "bot_admin: UPDATE で client_secret/private_key を更新する",
        {
            let _env = crate::common::ENV_LOCK.lock().unwrap();
            std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(
                &state.pool,
                &format!("BotUpd{}", uuid::Uuid::new_v4().simple()),
            )
            .await;
            let (user_id, _) = crate::common::create_test_user_in_db(
                &state.pool,
                tenant_id,
                "botupd@test.com",
                "admin",
            )
            .await;
            let jwt = crate::common::create_test_jwt_for_user(
                user_id,
                tenant_id,
                "botupd@test.com",
                "admin",
            );
            let client = reqwest::Client::new();

            // Step 1: CREATE a bot config (no id → INSERT path)
            let res = client
                .post(format!("{base_url}/api/admin/bot/configs"))
                .header("Authorization", format!("Bearer {jwt}"))
                .json(&serde_json::json!({
                    "name": "TestBot",
                    "client_id": "bot-client-id",
                    "client_secret": "initial-secret",
                    "service_account": "sa@test.com",
                    "private_key": "initial-pk",
                    "bot_id": "bot-123"
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let created: serde_json::Value = res.json().await.unwrap();
            let config_id = created["id"].as_str().unwrap().to_string();

            // Step 2: UPDATE with non-empty client_secret and private_key (lines 134, 148)
            let res = client
                .post(format!("{base_url}/api/admin/bot/configs"))
                .header("Authorization", format!("Bearer {jwt}"))
                .json(&serde_json::json!({
                    "id": config_id,
                    "name": "TestBotUpdated",
                    "client_id": "bot-client-id",
                    "client_secret": "updated-secret",
                    "service_account": "sa@test.com",
                    "private_key": "updated-pk",
                    "bot_id": "bot-123"
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let updated: serde_json::Value = res.json().await.unwrap();
            assert_eq!(updated["name"], "TestBotUpdated");

            // Cleanup
            sqlx::query("DELETE FROM alc_api.bot_configs WHERE id = $1::uuid")
                .bind(&config_id)
                .execute(&state.pool)
                .await
                .unwrap();
        }
    );
}

// ============================================================
// 5b. bot_admin: update WITHOUT secrets → None path (lines 132, 144)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_bot_admin_update_without_secrets() {
    test_group!("カバレッジ 100% 補完");
    test_case!(
        "bot_admin update: client_secret/private_key省略 → None分岐",
        {
            let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
            let _flock = crate::common::db_rename_flock();
            let _env = crate::common::ENV_LOCK.lock().unwrap();
            std::env::set_var("JWT_SECRET", crate::common::TEST_JWT_SECRET);
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(
                &state.pool,
                &format!("BotNone{}", uuid::Uuid::new_v4().simple()),
            )
            .await;
            let (user_id, _) = crate::common::create_test_user_in_db(
                &state.pool,
                tenant_id,
                "botnone@test.com",
                "admin",
            )
            .await;
            let jwt = crate::common::create_test_jwt_for_user(
                user_id,
                tenant_id,
                "botnone@test.com",
                "admin",
            );
            let client = reqwest::Client::new();

            // CREATE
            let res = client
                .post(format!("{base_url}/api/admin/bot/configs"))
                .header("Authorization", format!("Bearer {jwt}"))
                .json(&serde_json::json!({
                    "name": "BotNone",
                    "client_id": "none-client",
                    "client_secret": "init-secret",
                    "service_account": "sa@test.com",
                    "private_key": "init-pk",
                    "bot_id": "bot-none"
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let created: serde_json::Value = res.json().await.unwrap();
            let config_id = created["id"].as_str().unwrap().to_string();

            // UPDATE without client_secret and private_key → None path
            let res = client
                .post(format!("{base_url}/api/admin/bot/configs"))
                .header("Authorization", format!("Bearer {jwt}"))
                .json(&serde_json::json!({
                    "id": config_id,
                    "name": "BotNoneUpd",
                    "client_id": "none-client",
                    "service_account": "sa@test.com",
                    "bot_id": "bot-none"
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);

            // Cleanup
            sqlx::query("DELETE FROM alc_api.bot_configs WHERE id = $1::uuid")
                .bind(&config_id)
                .execute(&state.pool)
                .await
                .unwrap();
        }
    );
}

// ============================================================
// 6. tenko_schedules: batch empty array → 400
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_schedules_batch_empty() {
    test_group!("カバレッジ 100% 補完");
    test_case!("tenko_schedules batch_create: 空配列 → 400", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "TSchEmpty").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko/schedules/batch"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({ "schedules": [] }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });
}

// ============================================================
// 7. tenko_schedules: pre_operation without instruction → 400
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_schedules_pre_op_no_instruction() {
    test_group!("カバレッジ 100% 補完");
    test_case!(
        "tenko_schedules: pre_operation without instruction → 400",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(&state.pool, "TSchNoIns").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");

            let client = reqwest::Client::new();
            let res = client
                .post(format!("{base_url}/api/tenko/schedules/batch"))
                .header("Authorization", format!("Bearer {jwt}"))
                .json(&serde_json::json!({
                    "schedules": [{
                        "employee_id": "00000000-0000-0000-0000-000000000001",
                        "tenko_type": "pre_operation",
                        "responsible_manager_name": "Manager",
                        "scheduled_at": "2026-04-01T09:00:00Z"
                    }]
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 400);
        }
    );
}

// ============================================================
// 8. tenko_schedules: list with employee_id + tenko_type filters
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_schedules_list_with_filters() {
    test_group!("カバレッジ 100% 補完");
    test_case!(
        "tenko_schedules list: employee_id + tenko_type フィルタ",
        {
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(&state.pool, "TSchFilt").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");

            let emp_id = "00000000-0000-0000-0000-000000000099";
            let client = reqwest::Client::new();
            let res = client
                .get(format!(
                    "{base_url}/api/tenko/schedules?employee_id={emp_id}&tenko_type=pre_operation"
                ))
                .header("Authorization", format!("Bearer {jwt}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: serde_json::Value = res.json().await.unwrap();
            assert_eq!(body["total"], 0);
        }
    );
}

// ============================================================
// 9. tenko_schedules: batch_create DB error
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_schedules_batch_db_error() {
    test_group!("カバレッジ 100% 補完");
    test_case!("tenko_schedules batch_create: DB エラー → 500", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "TSchBErr").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        // Create trigger to block INSERT
        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.reject_sched_insert() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: schedule insert blocked'; END;
               $$ LANGUAGE plpgsql"#,
        )
        .execute(&state.pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER reject_sched_insert BEFORE INSERT ON alc_api.tenko_schedules \
             FOR EACH ROW EXECUTE FUNCTION alc_api.reject_sched_insert()",
        )
        .execute(&state.pool)
        .await
        .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko/schedules/batch"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({
                "schedules": [{
                    "employee_id": "00000000-0000-0000-0000-000000000001",
                    "tenko_type": "post_operation",
                    "responsible_manager_name": "Manager",
                    "scheduled_at": "2026-04-01T09:00:00Z"
                }]
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        // Cleanup
        sqlx::query("DROP TRIGGER reject_sched_insert ON alc_api.tenko_schedules")
            .execute(&state.pool)
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.reject_sched_insert()")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

// ============================================================
// 10. equipment_failures: list with session_id + date filters
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_equipment_failures_list_with_filters() {
    test_group!("カバレッジ 100% 補完");
    test_case!("equipment_failures list: session_id + date フィルタ", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "EqFltFilt").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        let session_id = "00000000-0000-0000-0000-000000000099";
        let client = reqwest::Client::new();
        let res = client
            .get(format!(
                "{base_url}/api/tenko/equipment-failures?session_id={session_id}&date_from=2026-01-01T00:00:00Z&date_to=2026-12-31T23:59:59Z"
            ))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: serde_json::Value = res.json().await.unwrap();
        assert_eq!(body["total"], 0);
    });
}

// ============================================================
// 11. equipment_failures: CSV export with date filters
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_equipment_failures_csv_with_date_filters() {
    test_group!("カバレッジ 100% 補完");
    test_case!("equipment_failures CSV: date フィルタ", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "EqCsvFlt").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        let client = reqwest::Client::new();
        let res = client
            .get(format!(
                "{base_url}/api/tenko/equipment-failures/csv?date_from=2026-01-01T00:00:00Z&date_to=2026-12-31T23:59:59Z"
            ))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let content_type = res.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(content_type.contains("text/csv"));
    });
}

// ============================================================
// 12. tenko_webhooks: invalid event_type → 400
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_webhooks_invalid_event_type() {
    test_group!("カバレッジ 100% 補完");
    test_case!("tenko_webhooks: invalid event_type → 400", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "WHInvEvt").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko/webhooks"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({
                "event_type": "invalid_event",
                "url": "https://example.com/webhook"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });
}

// ============================================================
// 13. tenko_webhooks: get_webhook success + delete not found
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_webhooks_get_and_delete_not_found() {
    test_group!("カバレッジ 100% 補完");
    test_case!("tenko_webhooks: GET成功 + DELETE not found → 404", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "WHGetDel").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        // Create a webhook config first
        let res = client
            .post(format!("{base_url}/api/tenko/webhooks"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({
                "event_type": "alcohol_detected",
                "url": "https://example.com/webhook",
                "secret": "test-secret"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);
        let created: serde_json::Value = res.json().await.unwrap();
        let webhook_id = created["id"].as_str().unwrap();

        // GET the webhook (covers lines 107-134)
        let res = client
            .get(format!("{base_url}/api/tenko/webhooks/{webhook_id}"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);

        // DELETE non-existent webhook → 404 (line 163)
        let fake_id = "00000000-0000-0000-0000-000000000099";
        let res = client
            .delete(format!("{base_url}/api/tenko/webhooks/{fake_id}"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);

        // Cleanup: delete the actual webhook
        sqlx::query("DELETE FROM alc_api.webhook_configs WHERE id = $1::uuid")
            .bind(webhook_id)
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

// ============================================================
// 14. tenko_webhooks: upsert DB error
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_webhooks_upsert_db_error() {
    test_group!("カバレッジ 100% 補完");
    test_case!("tenko_webhooks upsert: DB エラー → 500", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "WHUpsErr").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        // Create trigger to block INSERT on webhook_configs
        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.reject_wh_insert() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: webhook insert blocked'; END;
               $$ LANGUAGE plpgsql"#,
        )
        .execute(&state.pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER reject_wh_insert BEFORE INSERT ON alc_api.webhook_configs \
             FOR EACH ROW EXECUTE FUNCTION alc_api.reject_wh_insert()",
        )
        .execute(&state.pool)
        .await
        .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/tenko/webhooks"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({
                "event_type": "alcohol_detected",
                "url": "https://example.com/webhook"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        // Cleanup
        sqlx::query("DROP TRIGGER reject_wh_insert ON alc_api.webhook_configs")
            .execute(&state.pool)
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.reject_wh_insert()")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

// ============================================================
// 15. tenko_webhooks: delete DB error
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_webhooks_delete_db_error() {
    test_group!("カバレッジ 100% 補完");
    test_case!("tenko_webhooks delete: DB エラー → 500", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "WHDelErr").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        // RENAME webhook_configs to cause DB error
        sqlx::query("ALTER TABLE alc_api.webhook_configs RENAME TO webhook_configs_bak")
            .execute(&state.pool)
            .await
            .unwrap();

        let client = reqwest::Client::new();
        let fake_id = "00000000-0000-0000-0000-000000000001";
        let res = client
            .delete(format!("{base_url}/api/tenko/webhooks/{fake_id}"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        // Restore
        sqlx::query("ALTER TABLE alc_api.webhook_configs_bak RENAME TO webhook_configs")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

// ============================================================
// carins_files.rs — DB error tests (RENAME pattern)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_carins_files_list_db_error() {
    test_group!("carins_files カバレッジ");
    test_case!(
        "list_files / list_recent / list_not_attached DB エラー",
        {
            let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
            let _flock = crate::common::db_rename_flock();
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(&state.pool, "CFiles").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");

            // RENAME files table
            sqlx::query("ALTER TABLE alc_api.files RENAME TO files_bak")
                .execute(&state.pool)
                .await
                .unwrap();

            let client = reqwest::Client::new();

            // list_files
            let res = client
                .get(format!("{base_url}/api/files"))
                .header("Authorization", format!("Bearer {jwt}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 500);

            // list_recent
            let res = client
                .get(format!("{base_url}/api/files/recent"))
                .header("Authorization", format!("Bearer {jwt}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 500);

            // list_not_attached
            let res = client
                .get(format!("{base_url}/api/files/not-attached"))
                .header("Authorization", format!("Bearer {jwt}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 500);

            // Restore
            sqlx::query("ALTER TABLE alc_api.files_bak RENAME TO files")
                .execute(&state.pool)
                .await
                .unwrap();
        }
    );
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_carins_files_get_download_db_error() {
    test_group!("carins_files カバレッジ");
    test_case!("get_file / download_file DB エラー", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "CFGet").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        let fake_uuid = "00000000-0000-0000-0000-000000000099";

        // RENAME files table
        sqlx::query("ALTER TABLE alc_api.files RENAME TO files_bak")
            .execute(&state.pool)
            .await
            .unwrap();

        let client = reqwest::Client::new();

        // get_file
        let res = client
            .get(format!("{base_url}/api/files/{fake_uuid}"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        // download_file
        let res = client
            .get(format!("{base_url}/api/files/{fake_uuid}/download"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        // Restore
        sqlx::query("ALTER TABLE alc_api.files_bak RENAME TO files")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_carins_files_create_and_download() {
    test_group!("carins_files カバレッジ");
    test_case!("create_file + download (s3_key path)", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "CFCreate").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        let client = reqwest::Client::new();

        // Create a file via JSON API
        use base64::{engine::general_purpose::STANDARD, Engine};
        let content_b64 = STANDARD.encode(b"hello test file content");
        let res = client
            .post(format!("{base_url}/api/files"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({
                "filename": "test.txt",
                "type": "text/plain",
                "content": content_b64
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);
        let body: serde_json::Value = res.json().await.unwrap();
        let uuid = body["uuid"].as_str().unwrap();

        // Download the file (s3_key path → mock storage)
        let res = client
            .get(format!("{base_url}/api/files/{uuid}/download"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let data = res.bytes().await.unwrap();
        assert_eq!(data.as_ref(), b"hello test file content");
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_carins_files_create_db_error() {
    test_group!("carins_files カバレッジ");
    test_case!("create_file DB insert エラー", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "CFIns").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        // Create trigger to reject INSERT on files
        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.reject_files_insert() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: files insert blocked'; END;
               $$ LANGUAGE plpgsql"#,
        )
        .execute(&state.pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER reject_files_insert BEFORE INSERT ON alc_api.files \
             FOR EACH ROW EXECUTE FUNCTION alc_api.reject_files_insert()",
        )
        .execute(&state.pool)
        .await
        .unwrap();

        let client = reqwest::Client::new();
        use base64::{engine::general_purpose::STANDARD, Engine};
        let content_b64 = STANDARD.encode(b"data");
        let res = client
            .post(format!("{base_url}/api/files"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({
                "filename": "test.txt",
                "type": "text/plain",
                "content": content_b64
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        // Cleanup
        sqlx::query("DROP TRIGGER IF EXISTS reject_files_insert ON alc_api.files")
            .execute(&state.pool)
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION IF EXISTS alc_api.reject_files_insert()")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_carins_files_delete_not_found() {
    test_group!("carins_files カバレッジ");
    test_case!("delete / restore not found", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "CFDel").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        let client = reqwest::Client::new();
        let fake_uuid = "00000000-0000-0000-0000-000000000099";

        // delete not found
        let res = client
            .post(format!("{base_url}/api/files/{fake_uuid}/delete"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);

        // restore not found
        let res = client
            .post(format!("{base_url}/api/files/{fake_uuid}/restore"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_carins_files_download_not_found() {
    test_group!("carins_files カバレッジ");
    test_case!("download_file ファイルなし", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "CFDnf").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        let client = reqwest::Client::new();
        let fake_uuid = "00000000-0000-0000-0000-000000000099";

        // download non-existent file → 404
        let res = client
            .get(format!("{base_url}/api/files/{fake_uuid}/download"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_carins_files_download_storage_error() {
    test_group!("carins_files カバレッジ");
    test_case!(
        "download_file ストレージエラー (s3_key exists but not in mock)",
        {
            let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
            let _flock = crate::common::db_rename_flock();
            let state = crate::common::setup_app_state().await;
            let base_url = crate::common::spawn_test_server(state.clone()).await;
            let tenant_id = crate::common::create_test_tenant(&state.pool, "CFStErr").await;
            let jwt = crate::common::create_test_jwt(tenant_id, "admin");

            // Insert a file row with s3_key but don't upload to mock storage
            let file_uuid = uuid::Uuid::new_v4();
            sqlx::query(
            "INSERT INTO alc_api.files (uuid, tenant_id, filename, type, s3_key, storage_class, created_at, last_accessed_at) \
             VALUES ($1, $2, 'ghost.txt', 'text/plain', 'nonexistent/key', 'STANDARD', NOW(), NOW())"
        )
        .bind(file_uuid)
        .bind(tenant_id)
        .execute(&state.pool)
        .await
        .unwrap();

            // Set RLS tenant
            let client = reqwest::Client::new();
            let res = client
                .get(format!("{base_url}/api/files/{file_uuid}/download"))
                .header("Authorization", format!("Bearer {jwt}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 500);
        }
    );
}

// ============================================================
// measurements.rs — DB error + edge case tests
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_measurements_start_db_error() {
    test_group!("measurements カバレッジ");
    test_case!("start_measurement DB エラー", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "MStart").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        let emp = crate::common::create_test_employee(
            &reqwest::Client::new(),
            &base_url,
            &format!("Bearer {jwt}"),
            "MStartEmp",
            &format!("MS{}", &uuid::Uuid::new_v4().simple().to_string()[..4]),
        )
        .await;
        let emp_id = emp["id"].as_str().unwrap();

        // RENAME measurements table
        sqlx::query("ALTER TABLE alc_api.measurements RENAME TO measurements_bak")
            .execute(&state.pool)
            .await
            .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .post(format!("{base_url}/api/measurements/start"))
            .header("Authorization", format!("Bearer {jwt}"))
            .json(&serde_json::json!({ "employee_id": emp_id }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        // Restore
        sqlx::query("ALTER TABLE alc_api.measurements_bak RENAME TO measurements")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_measurements_update_deserialize_error() {
    test_group!("measurements カバレッジ");
    test_case!("update_measurement デシリアライズエラー", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "MUpDe").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        let client = reqwest::Client::new();
        let emp = crate::common::create_test_employee(
            &client,
            &base_url,
            &format!("Bearer {jwt}"),
            "MUpDeEmp",
            &format!("MD{}", &uuid::Uuid::new_v4().simple().to_string()[..4]),
        )
        .await;
        let emp_id = emp["id"].as_str().unwrap();

        // Create a measurement first
        let m = crate::common::create_test_measurement(
            &client,
            &base_url,
            &format!("Bearer {jwt}"),
            emp_id,
        )
        .await;
        let mid = m["id"].as_str().unwrap();

        // Send malformed JSON to update → 422
        let res = client
            .put(format!("{base_url}/api/measurements/{mid}"))
            .header("Authorization", format!("Bearer {jwt}"))
            .header("Content-Type", "application/json")
            .body(r#"{"alcohol_value": "not-a-number-but-expected-float"}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 422);
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_measurements_update_db_error() {
    test_group!("measurements カバレッジ");
    test_case!("update_measurement DB エラー", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "MUpDb").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        let client = reqwest::Client::new();
        let emp = crate::common::create_test_employee(
            &client,
            &base_url,
            &format!("Bearer {jwt}"),
            "MUpDbEmp",
            &format!("MU{}", &uuid::Uuid::new_v4().simple().to_string()[..4]),
        )
        .await;
        let emp_id = emp["id"].as_str().unwrap();
        let m = crate::common::create_test_measurement(
            &client,
            &base_url,
            &format!("Bearer {jwt}"),
            emp_id,
        )
        .await;
        let mid = m["id"].as_str().unwrap();

        // RENAME measurements
        sqlx::query("ALTER TABLE alc_api.measurements RENAME TO measurements_bak")
            .execute(&state.pool)
            .await
            .unwrap();

        let res = client
            .put(format!("{base_url}/api/measurements/{mid}"))
            .header("Authorization", format!("Bearer {jwt}"))
            .header("Content-Type", "application/json")
            .body(r#"{"status": "completed"}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        // Restore
        sqlx::query("ALTER TABLE alc_api.measurements_bak RENAME TO measurements")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_measurements_list_db_error() {
    test_group!("measurements カバレッジ");
    test_case!("list_measurements DB エラー (count + items)", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "MList").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        // RENAME measurements
        sqlx::query("ALTER TABLE alc_api.measurements RENAME TO measurements_bak")
            .execute(&state.pool)
            .await
            .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .get(format!("{base_url}/api/measurements"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        // Restore
        sqlx::query("ALTER TABLE alc_api.measurements_bak RENAME TO measurements")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_measurements_video_proxy() {
    test_group!("measurements カバレッジ");
    test_case!("get_video プロキシ (正常 + not found)", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "MVid").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let emp = crate::common::create_test_employee(
            &client,
            &base_url,
            &format!("Bearer {jwt}"),
            "MVideoEmp",
            &format!("MV{}", &uuid::Uuid::new_v4().simple().to_string()[..4]),
        )
        .await;
        let emp_id = emp["id"].as_str().unwrap();

        // Create measurement with video_url
        let m = crate::common::create_test_measurement(
            &client,
            &base_url,
            &format!("Bearer {jwt}"),
            emp_id,
        )
        .await;
        let mid = m["id"].as_str().unwrap();

        // Upload video to mock storage
        let video_key = format!("{tenant_id}/video-{mid}.webm");
        let video_url = format!("https://mock-storage/test-bucket/{video_key}");
        state
            .storage
            .upload(&video_key, b"fake-video-data", "video/webm")
            .await
            .unwrap();

        // Update measurement with video_url
        let res = client
            .put(format!("{base_url}/api/measurements/{mid}"))
            .header("Authorization", format!("Bearer {jwt}"))
            .header("Content-Type", "application/json")
            .body(serde_json::json!({ "video_url": video_url }).to_string())
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);

        // GET video proxy
        let res = client
            .get(format!("{base_url}/api/measurements/{mid}/video"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let data = res.bytes().await.unwrap();
        assert_eq!(data.as_ref(), b"fake-video-data");

        // GET video for measurement without video_url → 404
        let m2 = crate::common::create_test_measurement(
            &client,
            &base_url,
            &format!("Bearer {jwt}"),
            emp_id,
        )
        .await;
        let mid2 = m2["id"].as_str().unwrap();
        let res = client
            .get(format!("{base_url}/api/measurements/{mid2}/video"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 404);
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_measurements_video_db_error() {
    test_group!("measurements カバレッジ");
    test_case!("get_video DB エラー", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "MVErr").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        // RENAME measurements
        sqlx::query("ALTER TABLE alc_api.measurements RENAME TO measurements_bak")
            .execute(&state.pool)
            .await
            .unwrap();

        let client = reqwest::Client::new();
        let fake_id = "00000000-0000-0000-0000-000000000001";
        let res = client
            .get(format!("{base_url}/api/measurements/{fake_id}/video"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        // Restore
        sqlx::query("ALTER TABLE alc_api.measurements_bak RENAME TO measurements")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_measurements_create_kiosk_db_error() {
    test_group!("measurements カバレッジ");
    test_case!("create_measurement (kiosk) DB エラー", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "MKiosk").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        let client = reqwest::Client::new();
        let emp = crate::common::create_test_employee(
            &client,
            &base_url,
            &format!("Bearer {jwt}"),
            "MKioskEmp",
            &format!("MK{}", &uuid::Uuid::new_v4().simple().to_string()[..4]),
        )
        .await;
        let emp_id = emp["id"].as_str().unwrap();

        // RENAME measurements
        sqlx::query("ALTER TABLE alc_api.measurements RENAME TO measurements_bak")
            .execute(&state.pool)
            .await
            .unwrap();

        let res = client
            .post(format!("{base_url}/api/measurements"))
            .header("Authorization", format!("Bearer {jwt}"))
            .header("Content-Type", "application/json")
            .body(
                serde_json::json!({
                    "employee_id": emp_id,
                    "alcohol_value": 0.0,
                    "result_type": "pass"
                })
                .to_string(),
            )
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        // Restore
        sqlx::query("ALTER TABLE alc_api.measurements_bak RENAME TO measurements")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

// ============================================================
// tenko_records.rs — filter + CSV with Phase 2 JSONB data
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_records_csv_with_filters_and_phase2() {
    test_group!("tenko_records カバレッジ");
    test_case!("CSV export with tenko_type/status filter + self_declaration/daily_inspection/safety_judgment", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "TRCsv").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let emp = crate::common::create_test_employee(
            &client,
            &base_url,
            &format!("Bearer {jwt}"),
            "TRCsvEmp",
            &format!("TR{}", &uuid::Uuid::new_v4().simple().to_string()[..4]),
        )
        .await;
        let emp_id = emp["id"].as_str().unwrap();

        // Create a tenko_session (needed as FK for tenko_records)
        let session_id = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO alc_api.tenko_sessions (id, tenant_id, employee_id, tenko_type, status) \
             VALUES ($1, $2, $3::uuid, 'pre_operation', 'completed')"
        )
        .bind(session_id)
        .bind(tenant_id)
        .bind(emp_id)
        .execute(&state.pool)
        .await
        .unwrap();

        // Insert a tenko_record with Phase 2 JSONB data directly
        let self_declaration = serde_json::json!({
            "illness": true,
            "fatigue": false,
            "sleep_deprivation": true
        });
        let safety_judgment = serde_json::json!({
            "status": "failed",
            "failed_items": ["temperature", "alcohol"]
        });
        let daily_inspection = serde_json::json!({
            "brakes": "ok",
            "tires": "ok",
            "lights": "ng",
            "steering": "ok",
            "wipers": "ok",
            "mirrors": "ok",
            "horn": "ok",
            "seatbelts": "ok"
        });

        sqlx::query(
            r#"INSERT INTO alc_api.tenko_records (
                tenant_id, session_id, employee_id, tenko_type, status, record_data,
                employee_name, responsible_manager_name, tenko_method,
                alcohol_result, alcohol_value, alcohol_has_face_photo,
                started_at, completed_at, record_hash,
                self_declaration, safety_judgment, daily_inspection
            ) VALUES (
                $1, $2, $3::uuid, 'pre_operation', 'completed', '{}',
                'TRCsvEmp', '管理者', '自動点呼',
                'pass', 0.0, false,
                NOW(), NOW(), 'hash123',
                $4, $5, $6
            )"#
        )
        .bind(tenant_id)
        .bind(session_id)
        .bind(emp_id)
        .bind(&self_declaration)
        .bind(&safety_judgment)
        .bind(&daily_inspection)
        .execute(&state.pool)
        .await
        .unwrap();

        // CSV export with tenko_type filter
        let res = client
            .get(format!(
                "{base_url}/api/tenko/records/csv?tenko_type=pre_operation&status=completed"
            ))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let csv_bytes = res.bytes().await.unwrap();
        let csv_text = String::from_utf8_lossy(&csv_bytes);

        // Verify Phase 2 data in CSV
        assert!(csv_text.contains("true"), "CSV should contain self_declaration illness=true");
        assert!(csv_text.contains("false"), "CSV should contain self_declaration fatigue=false");
        assert!(csv_text.contains("failed"), "CSV should contain safety_judgment status=failed");
        assert!(csv_text.contains("temperature;alcohol"), "CSV should contain failed_items");
        assert!(csv_text.contains("ng"), "CSV should contain daily_inspection ng status");

        // Also test list with both filters (tenko_type + status)
        let res = client
            .get(format!(
                "{base_url}/api/tenko/records?tenko_type=pre_operation&status=completed"
            ))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: serde_json::Value = res.json().await.unwrap();
        assert!(body["total"].as_i64().unwrap() >= 1);
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_tenko_records_csv_all_ok_inspection() {
    test_group!("tenko_records カバレッジ");
    test_case!("CSV export with daily_inspection all ok", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "TRCok").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let client = reqwest::Client::new();

        let emp = crate::common::create_test_employee(
            &client,
            &base_url,
            &format!("Bearer {jwt}"),
            "TROkEmp",
            &format!("TO{}", &uuid::Uuid::new_v4().simple().to_string()[..4]),
        )
        .await;
        let emp_id = emp["id"].as_str().unwrap();

        let session_id = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO alc_api.tenko_sessions (id, tenant_id, employee_id, tenko_type, status) \
             VALUES ($1, $2, $3::uuid, 'pre_operation', 'completed')",
        )
        .bind(session_id)
        .bind(tenant_id)
        .bind(emp_id)
        .execute(&state.pool)
        .await
        .unwrap();

        // daily_inspection: all ok → "ok" status
        let daily_inspection = serde_json::json!({
            "brakes": "ok", "tires": "ok", "lights": "ok", "steering": "ok",
            "wipers": "ok", "mirrors": "ok", "horn": "ok", "seatbelts": "ok"
        });

        sqlx::query(
            r#"INSERT INTO alc_api.tenko_records (
                tenant_id, session_id, employee_id, tenko_type, status, record_data,
                employee_name, responsible_manager_name, tenko_method,
                alcohol_has_face_photo, record_hash,
                daily_inspection
            ) VALUES (
                $1, $2, $3::uuid, 'pre_operation', 'completed', '{}',
                'TROkEmp', '管理者', '自動点呼',
                false, 'hash456',
                $4
            )"#,
        )
        .bind(tenant_id)
        .bind(session_id)
        .bind(emp_id)
        .bind(&daily_inspection)
        .execute(&state.pool)
        .await
        .unwrap();

        let res = client
            .get(format!("{base_url}/api/tenko/records/csv"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let csv_bytes = res.bytes().await.unwrap();
        let csv_text = String::from_utf8_lossy(&csv_bytes);
        // daily_inspection_status should be "ok" (all items ok)
        assert!(
            csv_text.contains(",ok,"),
            "CSV should contain daily_inspection_status=ok"
        );
    });
}

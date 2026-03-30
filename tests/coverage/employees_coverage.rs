use crate::common;

// ============================================================
// Employees DB error injection tests
// ============================================================

// ============================================================
// create_employee: INSERT trigger → 500
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_create_employee_db_error() {
    test_group!("employees カバレッジ");
    test_case!("create_employee: INSERT trigger → 500", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(state.pool(), "EmpCreateErr").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.reject_emp_insert() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: employees insert blocked'; END;
               $$ LANGUAGE plpgsql"#,
        )
        .execute(state.pool())
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER reject_emp_insert BEFORE INSERT ON alc_api.employees \
             FOR EACH ROW EXECUTE FUNCTION alc_api.reject_emp_insert()",
        )
        .execute(state.pool())
        .await
        .unwrap();

        let res = client
            .post(format!("{base_url}/api/employees"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "name": "ErrEmp",
                "code": "ERR01"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("DROP TRIGGER reject_emp_insert ON alc_api.employees")
            .execute(state.pool())
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.reject_emp_insert()")
            .execute(state.pool())
            .await
            .unwrap();
    });
}

// ============================================================
// delete_employee: UPDATE trigger → 500 (soft delete = UPDATE)
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_delete_employee_db_error() {
    test_group!("employees カバレッジ");
    test_case!("delete_employee: UPDATE trigger → 500", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(state.pool(), "EmpDelErr").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        // Create employee first
        let emp =
            common::create_test_employee(&client, &base_url, &auth, "DelErrEmp", "DE01").await;
        let id = emp["id"].as_str().unwrap();

        // Trigger: block UPDATE on employees
        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.reject_emp_update() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: employees update blocked'; END;
               $$ LANGUAGE plpgsql"#,
        )
        .execute(state.pool())
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER reject_emp_update BEFORE UPDATE ON alc_api.employees \
             FOR EACH ROW EXECUTE FUNCTION alc_api.reject_emp_update()",
        )
        .execute(state.pool())
        .await
        .unwrap();

        let res = client
            .delete(format!("{base_url}/api/employees/{id}"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("DROP TRIGGER reject_emp_update ON alc_api.employees")
            .execute(state.pool())
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.reject_emp_update()")
            .execute(state.pool())
            .await
            .unwrap();
    });
}

// ============================================================
// update_face: UPDATE trigger → 500
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_update_face_db_error() {
    test_group!("employees カバレッジ");
    test_case!("update_face: UPDATE trigger → 500", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(state.pool(), "FaceErr").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let emp =
            common::create_test_employee(&client, &base_url, &auth, "FaceErrEmp", "FE01").await;
        let id = emp["id"].as_str().unwrap();

        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.reject_emp_update_face() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: employees update blocked'; END;
               $$ LANGUAGE plpgsql"#,
        )
        .execute(state.pool())
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER reject_emp_update_face BEFORE UPDATE ON alc_api.employees \
             FOR EACH ROW EXECUTE FUNCTION alc_api.reject_emp_update_face()",
        )
        .execute(state.pool())
        .await
        .unwrap();

        let embedding: Vec<f64> = vec![0.1; 1024];
        let res = client
            .put(format!("{base_url}/api/employees/{id}/face"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "face_embedding": embedding,
                "face_model_version": "v1"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("DROP TRIGGER reject_emp_update_face ON alc_api.employees")
            .execute(state.pool())
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.reject_emp_update_face()")
            .execute(state.pool())
            .await
            .unwrap();
    });
}

// ============================================================
// update_license: UPDATE trigger → 500
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_update_license_db_error() {
    test_group!("employees カバレッジ");
    test_case!("update_license: UPDATE trigger → 500", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(state.pool(), "LicErr").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let emp =
            common::create_test_employee(&client, &base_url, &auth, "LicErrEmp", "LE01").await;
        let id = emp["id"].as_str().unwrap();

        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.reject_emp_update_lic() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: employees update blocked'; END;
               $$ LANGUAGE plpgsql"#,
        )
        .execute(state.pool())
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER reject_emp_update_lic BEFORE UPDATE ON alc_api.employees \
             FOR EACH ROW EXECUTE FUNCTION alc_api.reject_emp_update_lic()",
        )
        .execute(state.pool())
        .await
        .unwrap();

        let res = client
            .put(format!("{base_url}/api/employees/{id}/license"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "license_issue_date": "2025-01-01",
                "license_expiry_date": "2028-01-01"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("DROP TRIGGER reject_emp_update_lic ON alc_api.employees")
            .execute(state.pool())
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.reject_emp_update_lic()")
            .execute(state.pool())
            .await
            .unwrap();
    });
}

// ============================================================
// update_nfc_id: UPDATE trigger → 500
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_update_nfc_id_db_error() {
    test_group!("employees カバレッジ");
    test_case!("update_nfc_id: UPDATE trigger → 500", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(state.pool(), "NfcErr").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let emp =
            common::create_test_employee(&client, &base_url, &auth, "NfcErrEmp", "NE01").await;
        let id = emp["id"].as_str().unwrap();

        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.reject_emp_update_nfc() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: employees update blocked'; END;
               $$ LANGUAGE plpgsql"#,
        )
        .execute(state.pool())
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER reject_emp_update_nfc BEFORE UPDATE ON alc_api.employees \
             FOR EACH ROW EXECUTE FUNCTION alc_api.reject_emp_update_nfc()",
        )
        .execute(state.pool())
        .await
        .unwrap();

        let res = client
            .put(format!("{base_url}/api/employees/{id}/nfc"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "nfc_id": "nfc-err-test"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("DROP TRIGGER reject_emp_update_nfc ON alc_api.employees")
            .execute(state.pool())
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.reject_emp_update_nfc()")
            .execute(state.pool())
            .await
            .unwrap();
    });
}

// ============================================================
// approve_face: UPDATE trigger → 500
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_approve_face_db_error() {
    test_group!("employees カバレッジ");
    test_case!("approve_face: UPDATE trigger → 500", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(state.pool(), "AppFaceErr").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let emp =
            common::create_test_employee(&client, &base_url, &auth, "AppFaceEmp", "AF01").await;
        let id = emp["id"].as_str().unwrap();

        // Set face to pending status first (need embedding)
        let embedding: Vec<f64> = vec![0.1; 1024];
        let res = client
            .put(format!("{base_url}/api/employees/{id}/face"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "face_embedding": embedding,
                "face_model_version": "v1"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);

        // Now block UPDATE
        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.reject_emp_update_appr() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: employees update blocked'; END;
               $$ LANGUAGE plpgsql"#,
        )
        .execute(state.pool())
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER reject_emp_update_appr BEFORE UPDATE ON alc_api.employees \
             FOR EACH ROW EXECUTE FUNCTION alc_api.reject_emp_update_appr()",
        )
        .execute(state.pool())
        .await
        .unwrap();

        let res = client
            .put(format!("{base_url}/api/employees/{id}/face/approve"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("DROP TRIGGER reject_emp_update_appr ON alc_api.employees")
            .execute(state.pool())
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.reject_emp_update_appr()")
            .execute(state.pool())
            .await
            .unwrap();
    });
}

// ============================================================
// reject_face: UPDATE trigger → 500
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_reject_face_db_error() {
    test_group!("employees カバレッジ");
    test_case!("reject_face: UPDATE trigger → 500", {
        let _db = common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_id = common::create_test_tenant(state.pool(), "RejFaceErr").await;
        let jwt = common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let emp =
            common::create_test_employee(&client, &base_url, &auth, "RejFaceEmp", "RF01").await;
        let id = emp["id"].as_str().unwrap();

        // Set face to pending status first
        let embedding: Vec<f64> = vec![0.1; 1024];
        let res = client
            .put(format!("{base_url}/api/employees/{id}/face"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "face_embedding": embedding,
                "face_model_version": "v1"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);

        // Now block UPDATE
        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.reject_emp_update_rej() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: employees update blocked'; END;
               $$ LANGUAGE plpgsql"#,
        )
        .execute(state.pool())
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER reject_emp_update_rej BEFORE UPDATE ON alc_api.employees \
             FOR EACH ROW EXECUTE FUNCTION alc_api.reject_emp_update_rej()",
        )
        .execute(state.pool())
        .await
        .unwrap();

        let res = client
            .put(format!("{base_url}/api/employees/{id}/face/reject"))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("DROP TRIGGER reject_emp_update_rej ON alc_api.employees")
            .execute(state.pool())
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.reject_emp_update_rej()")
            .execute(state.pool())
            .await
            .unwrap();
    });
}

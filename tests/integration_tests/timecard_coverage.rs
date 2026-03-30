use crate::common;

// ============================================================
// Timecard DB error injection tests
// ============================================================

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_timecard_punch_db_error() {
    test_group!("timecard カバレッジ");
    test_case!("punch: time_punches INSERT trigger → 500", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "TcPunchErr").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        // Create employee + card for punch to find
        let emp =
            crate::common::create_test_employee(&client, &base_url, &auth, "PunchErrEmp", "PE01")
                .await;
        let emp_id = emp["id"].as_str().unwrap();

        let res = client
            .post(format!("{base_url}/api/timecard/cards"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "employee_id": emp_id,
                "card_id": "PUNCH-ERR-CARD",
                "label": "test"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 201);

        // Trigger: block INSERT on time_punches
        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.reject_time_punch() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: time_punches insert blocked'; END;
               $$ LANGUAGE plpgsql"#,
        )
        .execute(state.pool())
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER reject_time_punch BEFORE INSERT ON alc_api.time_punches \
             FOR EACH ROW EXECUTE FUNCTION alc_api.reject_time_punch()",
        )
        .execute(state.pool())
        .await
        .unwrap();

        let res = client
            .post(format!("{base_url}/api/timecard/punch"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "card_id": "PUNCH-ERR-CARD"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        // Cleanup
        sqlx::query("DROP TRIGGER reject_time_punch ON alc_api.time_punches")
            .execute(state.pool())
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.reject_time_punch()")
            .execute(state.pool())
            .await
            .unwrap();
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_timecard_create_card_db_error() {
    test_group!("timecard カバレッジ");
    test_case!("create_card: timecard_cards INSERT trigger → 500", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "TcCardErr").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let emp =
            crate::common::create_test_employee(&client, &base_url, &auth, "CardErrEmp", "CE01")
                .await;
        let emp_id = emp["id"].as_str().unwrap();

        // Trigger: block INSERT on timecard_cards
        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION alc_api.reject_timecard_card() RETURNS trigger AS $$
               BEGIN RAISE EXCEPTION 'test: timecard_cards insert blocked'; END;
               $$ LANGUAGE plpgsql"#,
        )
        .execute(state.pool())
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER reject_timecard_card BEFORE INSERT ON alc_api.timecard_cards \
             FOR EACH ROW EXECUTE FUNCTION alc_api.reject_timecard_card()",
        )
        .execute(state.pool())
        .await
        .unwrap();

        let res = client
            .post(format!("{base_url}/api/timecard/cards"))
            .header("Authorization", &auth)
            .json(&serde_json::json!({
                "employee_id": emp_id,
                "card_id": "ERR-CARD-001",
                "label": "test"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        // Cleanup
        sqlx::query("DROP TRIGGER reject_timecard_card ON alc_api.timecard_cards")
            .execute(state.pool())
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION alc_api.reject_timecard_card()")
            .execute(state.pool())
            .await
            .unwrap();
    });
}

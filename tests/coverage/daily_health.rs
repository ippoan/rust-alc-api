/// daily_health: DBエラーパス + safety_judgment pass/fail カウント

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_daily_health_db_error() {
    test_group!("カバレッジ 100% 補完");
    test_case!("daily-health-status で DB エラー時に 500 を返す", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "DHErr").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");

        // employees テーブルを RENAME してクエリエラーを発生させる
        // (ENV_LOCK で他テストと直列化)
        sqlx::query("ALTER TABLE alc_api.employees RENAME TO employees_cov_bak")
            .execute(&state.pool)
            .await
            .unwrap();

        let client = reqwest::Client::new();
        let res = client
            .get(format!("{base_url}/api/tenko/daily-health-status"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 500);

        sqlx::query("ALTER TABLE alc_api.employees_cov_bak RENAME TO employees")
            .execute(&state.pool)
            .await
            .unwrap();
    });
}

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_daily_health_safety_judgment_counts() {
    test_group!("カバレッジ 100% 補完");
    test_case!("safety_judgment の pass/fail カウントが正しい", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(&state.pool, "DHJudge").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let emp1 =
            crate::common::create_test_employee(&client, &base_url, &auth, "PassEmp", "PE01").await;
        let emp1_id: uuid::Uuid = emp1["id"].as_str().unwrap().parse().unwrap();

        let emp2 =
            crate::common::create_test_employee(&client, &base_url, &auth, "FailEmp", "FE01").await;
        let emp2_id: uuid::Uuid = emp2["id"].as_str().unwrap().parse().unwrap();

        crate::common::create_test_employee(&client, &base_url, &auth, "UncheckedEmp", "UE01")
            .await;

        {
            let mut conn = state.pool.acquire().await.unwrap();
            sqlx::query("SELECT set_config('app.current_tenant_id', $1, true)")
                .bind(tenant_id.to_string())
                .execute(&mut *conn)
                .await
                .unwrap();

            sqlx::query(
                r#"INSERT INTO alc_api.tenko_sessions
                   (tenant_id, employee_id, tenko_type, status, completed_at, safety_judgment)
                   VALUES ($1, $2, 'pre_operation', 'completed',
                           '2026-03-27T01:00:00Z', '{"status":"pass"}'::jsonb)"#,
            )
            .bind(tenant_id)
            .bind(emp1_id)
            .execute(&mut *conn)
            .await
            .unwrap();

            sqlx::query(
                r#"INSERT INTO alc_api.tenko_sessions
                   (tenant_id, employee_id, tenko_type, status, completed_at, safety_judgment)
                   VALUES ($1, $2, 'pre_operation', 'completed',
                           '2026-03-27T01:00:00Z', '{"status":"fail"}'::jsonb)"#,
            )
            .bind(tenant_id)
            .bind(emp2_id)
            .execute(&mut *conn)
            .await
            .unwrap();
        }

        let res = client
            .get(format!(
                "{base_url}/api/tenko/daily-health-status?date=2026-03-27"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: serde_json::Value = res.json().await.unwrap();
        let summary = &body["summary"];
        assert_eq!(summary["total_employees"], 3);
        assert_eq!(summary["pass_count"], 1);
        assert_eq!(summary["fail_count"], 1);
        assert_eq!(summary["unchecked_count"], 1);
        assert_eq!(summary["checked_count"], 2);
    });
}

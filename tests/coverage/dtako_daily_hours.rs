/// dtako_daily_hours: get_daily_segments エンドポイント

#[cfg_attr(not(coverage), ignore)]
#[tokio::test]
async fn test_daily_segments() {
    test_group!("日別セグメント (カバレッジ)");
    test_case!("セグメント一覧を取得できる", {
        let _db = crate::common::DB_RENAME_LOCK.lock().unwrap();
        let _flock = crate::common::db_rename_flock();
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "DtakoSeg").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let emp =
            crate::common::create_test_employee(&client, &base_url, &auth, "SegDriver", "SG01")
                .await;
        let emp_id: uuid::Uuid = emp["id"].as_str().unwrap().parse().unwrap();

        {
            let mut conn = state.pool().acquire().await.unwrap();
            sqlx::query("SELECT set_config('app.current_tenant_id', $1, true)")
                .bind(tenant_id.to_string())
                .execute(&mut *conn)
                .await
                .unwrap();

            let work_date = chrono::NaiveDate::from_ymd_opt(2026, 3, 27).unwrap();
            let start_at = work_date.and_hms_opt(8, 0, 0).unwrap().and_utc();
            let end_at = work_date.and_hms_opt(13, 0, 0).unwrap().and_utc();

            sqlx::query(
                r#"INSERT INTO alc_api.dtako_daily_work_segments
                   (tenant_id, driver_id, work_date, unko_no, segment_index,
                    start_at, end_at, work_minutes, labor_minutes, late_night_minutes,
                    drive_minutes, cargo_minutes)
                   VALUES ($1, $2, $3, 'OP001', 0, $4, $5, 300, 225, 0, 150, 75)"#,
            )
            .bind(tenant_id)
            .bind(emp_id)
            .bind(work_date)
            .bind(start_at)
            .bind(end_at)
            .execute(&mut *conn)
            .await
            .unwrap();
        }

        let res = client
            .get(format!(
                "{base_url}/api/daily-hours/{emp_id}/2026-03-27/segments"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: serde_json::Value = res.json().await.unwrap();
        let segments = body["segments"].as_array().unwrap();
        assert_eq!(segments.len(), 1);
    });
    test_case!("データなしで空配列を返す", {
        let state = crate::common::setup_app_state().await;
        let base_url = crate::common::spawn_test_server(state.clone()).await;
        let tenant_id = crate::common::create_test_tenant(state.pool(), "DtakoSegE").await;
        let jwt = crate::common::create_test_jwt(tenant_id, "admin");
        let auth = format!("Bearer {jwt}");
        let client = reqwest::Client::new();

        let emp =
            crate::common::create_test_employee(&client, &base_url, &auth, "SegEmpty", "SE01")
                .await;
        let emp_id = emp["id"].as_str().unwrap();

        let res = client
            .get(format!(
                "{base_url}/api/daily-hours/{emp_id}/2026-01-01/segments"
            ))
            .header("Authorization", &auth)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let body: serde_json::Value = res.json().await.unwrap();
        let segments = body["segments"].as_array().unwrap();
        assert!(segments.is_empty());
    });
}

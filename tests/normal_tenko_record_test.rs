//! 通常点呼 (運行者端末の測定) が点呼セッション・点呼記録として残ることを実 DB で固定する。
//!
//! Refs ippoan/alc-app#238, ippoan/alc-app-s3#135
//!
//! **実 DB でしか検証できない** — 冪等の担保は migration 140 の部分 unique
//! (`tenko_type = 'normal'` の measurement_id / (employee_id, started_at)) と
//! `ON CONFLICT DO NOTHING` に任せているので、repository を差し替える mock テストでは
//! 1 行も通らない。執行者 NULL の記録を一覧・CSV が壊れずに返せることも同じ理由で実 DB 用。
//!
//! 回し方は他の DB integration テストと同じ:
//!   `make db-up && source .test-config && cargo test --test normal_tenko_record_test`
//! CI は ci.yml の `bazel-test-db` shard (postgres service + TEST_DATABASE_URL)。

#[macro_use]
mod common;

use serde_json::Value;
use uuid::Uuid;

/// 測定を 1 件保存する (`record_as_tenko` の有無と結果を指定できる)
async fn post_measurement(
    client: &reqwest::Client,
    base_url: &str,
    auth: &str,
    employee_id: &str,
    result_type: &str,
    measured_at: &str,
    record_as_tenko: bool,
) -> Value {
    let res = client
        .post(format!("{base_url}/api/measurements"))
        .header("Authorization", auth)
        .json(&serde_json::json!({
            "employee_id": employee_id,
            "alcohol_value": 0.0,
            "result_type": result_type,
            "measured_at": measured_at,
            "temperature": 36.5,
            "systolic": 120,
            "diastolic": 80,
            "pulse": 64,
            "record_as_tenko": record_as_tenko,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201, "measurement POST failed");
    res.json().await.unwrap()
}

/// 通常点呼のセッションを測定 id で引く
async fn sessions_for_measurement(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    measurement_id: &str,
) -> Vec<(String, String, Option<String>)> {
    let mid = Uuid::parse_str(measurement_id).unwrap();
    sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT tenko_type, status, cancel_reason FROM alc_api.tenko_sessions
         WHERE tenant_id = $1 AND measurement_id = $2",
    )
    .bind(tenant_id)
    .bind(mid)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// テナント内の点呼セッション / 点呼記録の件数
async fn counts(pool: &sqlx::PgPool, tenant_id: Uuid) -> (i64, i64) {
    let sessions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM alc_api.tenko_sessions WHERE tenant_id = $1 AND tenko_type = 'normal'",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .unwrap();
    let records: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM alc_api.tenko_records WHERE tenant_id = $1 AND tenko_method = '通常点呼'",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .unwrap();
    (sessions, records)
}

#[tokio::test]
async fn test_normal_measurement_creates_tenko_session_and_record() {
    test_group!("通常点呼 → 点呼記録");
    test_case!(
        "印つきの測定で session と record が 1 件ずつできる",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant = common::create_test_tenant(state.pool(), "Normal Tenko Create").await;
            let jwt = common::create_test_jwt(tenant, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let emp =
                common::create_test_employee(&client, &base_url, &auth, "運行者", "NT1").await;
            let emp_id = emp["id"].as_str().unwrap();

            let m = post_measurement(
                &client,
                &base_url,
                &auth,
                emp_id,
                "normal",
                "2026-09-12T01:00:00Z",
                true,
            )
            .await;
            let m_id = m["id"].as_str().unwrap();

            // 測定は保存されている
            assert_eq!(m["status"], "completed");

            let sessions = sessions_for_measurement(state.pool(), tenant, m_id).await;
            assert_eq!(sessions.len(), 1, "通常点呼のセッションが 1 件できる");
            assert_eq!(sessions[0].0, "normal");
            assert_eq!(sessions[0].1, "completed");
            assert_eq!(sessions[0].2, None);

            // 点呼記録: 点呼方法 '通常点呼' / 執行者は空欄 / 体温・血圧が写っている
            let rec: (String, Option<String>, Option<f64>, Option<i32>, String) =
            sqlx::query_as(
                "SELECT tenko_method, responsible_manager_name, temperature, systolic, employee_name
                 FROM alc_api.tenko_records WHERE tenant_id = $1",
            )
            .bind(tenant)
            .fetch_one(state.pool())
            .await
            .unwrap();
            assert_eq!(rec.0, "通常点呼");
            assert_eq!(rec.1, None, "点呼執行者は空欄");
            assert_eq!(rec.2, Some(36.5));
            assert_eq!(rec.3, Some(120));
            assert_eq!(rec.4, "運行者");

            // 運行管理者の一覧に出る
            let res = client
                .get(format!("{base_url}/api/tenko/records"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["total"], 1);
            assert_eq!(body["records"][0]["tenko_method"], "通常点呼");
            assert!(body["records"][0]["responsible_manager_name"].is_null());

            // CSV も執行者が空欄のまま取れる
            let res = client
                .get(format!("{base_url}/api/tenko/records/csv"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(
                res.status(),
                200,
                "執行者 NULL の記録があっても CSV が取れる"
            );
            let csv = res.text().await.unwrap();
            assert!(csv.contains("通常点呼"), "CSV に通常点呼の行がある");

            // 点呼セッションの一覧にも出る
            let res = client
                .get(format!("{base_url}/api/tenko/sessions"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["total"], 1);
        }
    );
}

#[tokio::test]
async fn test_normal_measurement_is_idempotent() {
    test_group!("通常点呼 → 点呼記録");
    test_case!(
        "完了 PUT の 2 回目・同時刻の測定の 2 件目でも記録は 1 組",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant = common::create_test_tenant(state.pool(), "Normal Tenko Idempotent").await;
            let jwt = common::create_test_jwt(tenant, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let emp =
                common::create_test_employee(&client, &base_url, &auth, "運行者", "NT2").await;
            let emp_id = emp["id"].as_str().unwrap();
            let measured_at = "2026-09-12T02:00:00Z";

            let m = post_measurement(
                &client,
                &base_url,
                &auth,
                emp_id,
                "normal",
                measured_at,
                true,
            )
            .await;
            let m_id = m["id"].as_str().unwrap().to_string();
            assert_eq!(counts(state.pool(), tenant).await, (1, 1));

            // 同じ測定への完了 PUT を 2 回 (オンラインの再送)
            for _ in 0..2 {
                let res = client
                    .put(format!("{base_url}/api/measurements/{m_id}"))
                    .header("Authorization", &auth)
                    .json(&serde_json::json!({
                        "status": "completed",
                        "record_as_tenko": true,
                    }))
                    .send()
                    .await
                    .unwrap();
                assert_eq!(res.status(), 200);
            }
            assert_eq!(
                counts(state.pool(), tenant).await,
                (1, 1),
                "同じ測定への再送で記録は増えない"
            );

            // オンラインの完了 PUT が落ちて offline 保存へ回り、同じ乗務員・同じ測定時刻の
            // 測定がもう 1 件できる経路
            let m2 = post_measurement(
                &client,
                &base_url,
                &auth,
                emp_id,
                "normal",
                measured_at,
                true,
            )
            .await;
            assert_ne!(
                m2["id"].as_str().unwrap(),
                m_id,
                "測定自体は 2 件目ができる"
            );
            assert_eq!(
                counts(state.pool(), tenant).await,
                (1, 1),
                "同じ乗務員・同じ測定時刻なら記録は 1 組のまま"
            );
        }
    );
}

#[tokio::test]
async fn test_normal_measurement_over_is_cancelled() {
    test_group!("通常点呼 → 点呼記録");
    test_case!(
        "アルコール検知 (over) はセッションが中止になる",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant = common::create_test_tenant(state.pool(), "Normal Tenko Over").await;
            let jwt = common::create_test_jwt(tenant, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let emp =
                common::create_test_employee(&client, &base_url, &auth, "運行者", "NT3").await;
            let emp_id = emp["id"].as_str().unwrap();

            let m = post_measurement(
                &client,
                &base_url,
                &auth,
                emp_id,
                "over",
                "2026-09-12T03:00:00Z",
                true,
            )
            .await;
            let sessions =
                sessions_for_measurement(state.pool(), tenant, m["id"].as_str().unwrap()).await;
            assert_eq!(sessions.len(), 1);
            assert_eq!(sessions[0].1, "cancelled");
            assert_eq!(sessions[0].2.as_deref(), Some("アルコール検知"));
            assert_eq!(counts(state.pool(), tenant).await, (1, 1));
        }
    );
}

#[tokio::test]
async fn test_normal_measurement_error_result_is_not_recorded() {
    test_group!("通常点呼 → 点呼記録");
    test_case!(
        "結果が error の測定は保存だけされ、記録は作らない",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant = common::create_test_tenant(state.pool(), "Normal Tenko Error").await;
            let jwt = common::create_test_jwt(tenant, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let emp =
                common::create_test_employee(&client, &base_url, &auth, "運行者", "NT4").await;
            let emp_id = emp["id"].as_str().unwrap();

            let m = post_measurement(
                &client,
                &base_url,
                &auth,
                emp_id,
                "error",
                "2026-09-12T04:00:00Z",
                true,
            )
            .await;
            // 測定は保存される (エラーにしない)
            assert_eq!(m["status"], "completed");
            assert_eq!(counts(state.pool(), tenant).await, (0, 0));
        }
    );
}

#[tokio::test]
async fn test_measurement_without_mark_creates_nothing() {
    test_group!("通常点呼 → 点呼記録");
    test_case!(
        "印なしの測定は今までどおり記録を作らない",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant = common::create_test_tenant(state.pool(), "Normal Tenko Unmarked").await;
            let jwt = common::create_test_jwt(tenant, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let emp =
                common::create_test_employee(&client, &base_url, &auth, "運行者", "NT5").await;
            let emp_id = emp["id"].as_str().unwrap();

            post_measurement(
                &client,
                &base_url,
                &auth,
                emp_id,
                "normal",
                "2026-09-12T05:00:00Z",
                false,
            )
            .await;
            assert_eq!(counts(state.pool(), tenant).await, (0, 0));
        }
    );
}

#[tokio::test]
async fn test_measurement_survives_failed_record_creation() {
    test_group!("通常点呼 → 点呼記録");
    test_case!(
        "記録の作成が失敗しても測定は残る (内側だけ rollback)",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant = common::create_test_tenant(state.pool(), "Normal Tenko Failure").await;
            let jwt = common::create_test_jwt(tenant, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let emp =
                common::create_test_employee(&client, &base_url, &auth, "運行者", "NT6").await;
            let emp_id = emp["id"].as_str().unwrap();

            // 点呼記録の INSERT を**このテナントの行に限って**失敗させる。
            // 表の RENAME や DROP と違い、並行して走る他のテスト (別テナント) には当たらない。
            sqlx::query(
                r#"CREATE OR REPLACE FUNCTION alc_api.fail_tenko_record_insert_for_test()
               RETURNS TRIGGER AS $$
               BEGIN
                   RAISE EXCEPTION 'injected failure for test';
               END;
               $$ LANGUAGE plpgsql"#,
            )
            .execute(state.pool())
            .await
            .unwrap();
            sqlx::query(&format!(
                "CREATE TRIGGER trg_fail_tenko_record_insert_{}
             BEFORE INSERT ON alc_api.tenko_records
             FOR EACH ROW WHEN (NEW.tenant_id = '{tenant}')
             EXECUTE FUNCTION alc_api.fail_tenko_record_insert_for_test()",
                tenant.simple()
            ))
            .execute(state.pool())
            .await
            .unwrap();

            let m = post_measurement(
                &client,
                &base_url,
                &auth,
                emp_id,
                "normal",
                "2026-09-12T06:00:00Z",
                true,
            )
            .await;

            sqlx::query(&format!(
                "DROP TRIGGER trg_fail_tenko_record_insert_{} ON alc_api.tenko_records",
                tenant.simple()
            ))
            .execute(state.pool())
            .await
            .unwrap();

            // 測定は commit されている
            let res = client
                .get(format!(
                    "{base_url}/api/measurements/{}",
                    m["id"].as_str().unwrap()
                ))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200, "記録の作成が失敗しても測定は残る");

            // 記録側は内側の rollback でセッションごと巻き戻る
            assert_eq!(counts(state.pool(), tenant).await, (0, 0));
        }
    );
}

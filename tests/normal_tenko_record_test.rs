//! 通常点呼 (運行者端末の測定) が点呼セッション・点呼記録として残ることを実 DB で固定する。
//!
//! Refs ippoan/alc-app#238, ippoan/alc-app-s3#135
//!
//! **実 DB でしか検証できない** — 冪等の担保は migration 141 の部分 unique
//! (`tenko_type = 'normal' OR tenko_method = '通常点呼'` の measurement_id /
//! (employee_id, started_at)) と
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
    let res = send_measurement(
        client,
        base_url,
        auth,
        employee_id,
        result_type,
        measured_at,
        record_as_tenko,
        None,
    )
    .await;
    assert_eq!(res.status(), 201, "measurement POST failed");
    res.json().await.unwrap()
}

/// 測定の POST を送り、応答をそのまま返す (`tenko_type` を付けられる)
#[allow(clippy::too_many_arguments)]
async fn send_measurement(
    client: &reqwest::Client,
    base_url: &str,
    auth: &str,
    employee_id: &str,
    result_type: &str,
    measured_at: &str,
    record_as_tenko: bool,
    tenko_type: Option<&str>,
) -> reqwest::Response {
    let mut body = serde_json::json!({
        "employee_id": employee_id,
        "alcohol_value": 0.0,
        "result_type": result_type,
        "measured_at": measured_at,
        "temperature": 36.5,
        "systolic": 120,
        "diastolic": 80,
        "pulse": 64,
        "record_as_tenko": record_as_tenko,
    });
    if let Some(tt) = tenko_type {
        body["tenko_type"] = Value::from(tt);
    }
    client
        .post(format!("{base_url}/api/measurements"))
        .header("Authorization", auth)
        .json(&body)
        .send()
        .await
        .unwrap()
}

/// 測定に紐づく session / record の (tenko_type, tenko_method) を引く
async fn type_and_method(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    measurement_id: &str,
) -> (Vec<(String, String)>, Vec<(String, String)>) {
    let mid = Uuid::parse_str(measurement_id).unwrap();
    let sessions = sqlx::query_as::<_, (String, String)>(
        "SELECT tenko_type, tenko_method FROM alc_api.tenko_sessions
         WHERE tenant_id = $1 AND measurement_id = $2",
    )
    .bind(tenant_id)
    .bind(mid)
    .fetch_all(pool)
    .await
    .unwrap();
    let records = sqlx::query_as::<_, (String, String)>(
        "SELECT r.tenko_type, r.tenko_method FROM alc_api.tenko_records r
         JOIN alc_api.tenko_sessions s ON s.id = r.session_id
         WHERE r.tenant_id = $1 AND s.measurement_id = $2",
    )
    .bind(tenant_id)
    .bind(mid)
    .fetch_all(pool)
    .await
    .unwrap();
    (sessions, records)
}

/// テナント内の測定の件数
async fn measurement_count(pool: &sqlx::PgPool, tenant_id: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM alc_api.measurements WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_one(pool)
        .await
        .unwrap()
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
        "SELECT count(*) FROM alc_api.tenko_sessions WHERE tenant_id = $1 AND tenko_method = '通常点呼'",
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
            assert_eq!(
                type_and_method(state.pool(), tenant, m_id).await,
                (
                    vec![("normal".to_string(), "通常点呼".to_string())],
                    vec![("normal".to_string(), "通常点呼".to_string())],
                ),
                "tenko_type 無しは normal、session にも点呼方法 '通常点呼'"
            );

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

/// 始業 / 終業を選んだ測定が、その種別の点呼セッション・点呼記録になることを確かめる
async fn assert_typed_record(tenko_type: &str, tenant_name: &str, code: &str, measured_at: &str) {
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant = common::create_test_tenant(state.pool(), tenant_name).await;
    let jwt = common::create_test_jwt(tenant, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(&client, &base_url, &auth, "運行者", code).await;
    let emp_id = emp["id"].as_str().unwrap();

    let res = send_measurement(
        &client,
        &base_url,
        &auth,
        emp_id,
        "normal",
        measured_at,
        true,
        Some(tenko_type),
    )
    .await;
    assert_eq!(res.status(), 201);
    let m: Value = res.json().await.unwrap();

    let expected = vec![(tenko_type.to_string(), "通常点呼".to_string())];
    assert_eq!(
        type_and_method(state.pool(), tenant, m["id"].as_str().unwrap()).await,
        (expected.clone(), expected),
        "session・record とも tenko_type={tenko_type}、点呼方法は '通常点呼'"
    );
    assert_eq!(counts(state.pool(), tenant).await, (1, 1));
}

#[tokio::test]
async fn test_normal_measurement_with_pre_operation() {
    test_group!("通常点呼 → 点呼記録 (始業・終業)");
    test_case!("始業を選ぶと pre_operation で記録される", {
        assert_typed_record(
            "pre_operation",
            "Normal Tenko Pre",
            "NT7",
            "2026-09-12T07:00:00Z",
        )
        .await;
    });
}

#[tokio::test]
async fn test_normal_measurement_with_post_operation() {
    test_group!("通常点呼 → 点呼記録 (始業・終業)");
    test_case!("終業を選ぶと post_operation で記録される", {
        assert_typed_record(
            "post_operation",
            "Normal Tenko Post",
            "NT8",
            "2026-09-12T08:00:00Z",
        )
        .await;
    });
}

#[tokio::test]
async fn test_invalid_tenko_type_is_rejected() {
    test_group!("通常点呼 → 点呼記録 (始業・終業)");
    test_case!(
        "不正な種別は 400 で、測定も記録も作らない (POST / PUT)",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant = common::create_test_tenant(state.pool(), "Normal Tenko Invalid").await;
            let jwt = common::create_test_jwt(tenant, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let emp =
                common::create_test_employee(&client, &base_url, &auth, "運行者", "NT9").await;
            let emp_id = emp["id"].as_str().unwrap();

            // POST
            let res = send_measurement(
                &client,
                &base_url,
                &auth,
                emp_id,
                "normal",
                "2026-09-12T09:00:00Z",
                true,
                Some("mid_operation"),
            )
            .await;
            assert_eq!(res.status(), 400);
            assert_eq!(measurement_count(state.pool(), tenant).await, 0);
            assert_eq!(counts(state.pool(), tenant).await, (0, 0));

            // PUT (測定開始 → 完了 PUT に不正な種別)
            let res = client
                .post(format!("{base_url}/api/measurements/start"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "employee_id": emp_id }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let started: Value = res.json().await.unwrap();
            let m_id = started["id"].as_str().unwrap();

            let res = client
                .put(format!("{base_url}/api/measurements/{m_id}"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "status": "completed",
                    "alcohol_value": 0.0,
                    "result_type": "normal",
                    "measured_at": "2026-09-12T09:30:00Z",
                    "record_as_tenko": true,
                    "tenko_type": "pre",
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 400);

            let status: String =
                sqlx::query_scalar("SELECT status FROM alc_api.measurements WHERE id = $1")
                    .bind(Uuid::parse_str(m_id).unwrap())
                    .fetch_one(state.pool())
                    .await
                    .unwrap();
            assert_eq!(status, "started", "400 の PUT は測定を更新しない");
            assert_eq!(counts(state.pool(), tenant).await, (0, 0));
        }
    );
}

#[tokio::test]
async fn test_typed_record_is_idempotent() {
    test_group!("通常点呼 → 点呼記録 (始業・終業)");
    test_case!(
        "同じ測定へ pre の PUT 2 回・同じ乗務員同時刻の normal と post でも 1 組",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant = common::create_test_tenant(state.pool(), "Normal Tenko Typed Idem").await;
            let jwt = common::create_test_jwt(tenant, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let emp =
                common::create_test_employee(&client, &base_url, &auth, "運行者", "NT10").await;
            let emp_id = emp["id"].as_str().unwrap();

            // 同じ測定へ pre の完了 PUT を 2 回
            let m = post_measurement(
                &client,
                &base_url,
                &auth,
                emp_id,
                "normal",
                "2026-09-12T10:00:00Z",
                false,
            )
            .await;
            let m_id = m["id"].as_str().unwrap();
            for _ in 0..2 {
                let res = client
                    .put(format!("{base_url}/api/measurements/{m_id}"))
                    .header("Authorization", &auth)
                    .json(&serde_json::json!({
                        "status": "completed",
                        "record_as_tenko": true,
                        "tenko_type": "pre_operation",
                    }))
                    .send()
                    .await
                    .unwrap();
                assert_eq!(res.status(), 200);
            }
            assert_eq!(counts(state.pool(), tenant).await, (1, 1));
            let pre = vec![("pre_operation".to_string(), "通常点呼".to_string())];
            assert_eq!(
                type_and_method(state.pool(), tenant, m_id).await,
                (pre.clone(), pre)
            );

            // 同じ乗務員・同じ measured_at で normal → post
            let measured_at = "2026-09-12T11:00:00Z";
            post_measurement(
                &client,
                &base_url,
                &auth,
                emp_id,
                "normal",
                measured_at,
                true,
            )
            .await;
            let res = send_measurement(
                &client,
                &base_url,
                &auth,
                emp_id,
                "normal",
                measured_at,
                true,
                Some("post_operation"),
            )
            .await;
            assert_eq!(res.status(), 201, "測定自体は保存される");
            assert_eq!(
                counts(state.pool(), tenant).await,
                (2, 2),
                "同じ乗務員・同じ測定時刻の normal と post は 1 組 (先の 1 組と合わせて 2 組)"
            );
        }
    );
}

#[tokio::test]
async fn test_old_version_row_still_blocks_typed_resend() {
    test_group!("通常点呼 → 点呼記録 (始業・終業)");
    test_case!(
        "切り替えの窓: 旧版が作った (normal, DEFAULT) の session があれば pre の再送でも 1 組",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant = common::create_test_tenant(state.pool(), "Normal Tenko Window").await;
            let jwt = common::create_test_jwt(tenant, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let emp =
                common::create_test_employee(&client, &base_url, &auth, "運行者", "NT11").await;
            let emp_id = emp["id"].as_str().unwrap();

            let m = post_measurement(
                &client,
                &base_url,
                &auth,
                emp_id,
                "normal",
                "2026-09-12T12:00:00Z",
                false,
            )
            .await;
            let m_id = m["id"].as_str().unwrap();

            // 旧版の INSERT を模す: tenko_method を指定しない (DEFAULT '自動点呼' が入る)
            sqlx::query(
                "INSERT INTO alc_api.tenko_sessions (
                     tenant_id, employee_id, tenko_type, status, measurement_id,
                     started_at, completed_at
                 ) VALUES ($1, $2, 'normal', 'completed', $3, $4, NOW())",
            )
            .bind(tenant)
            .bind(Uuid::parse_str(emp_id).unwrap())
            .bind(Uuid::parse_str(m_id).unwrap())
            .bind(chrono::DateTime::parse_from_rfc3339("2026-09-12T12:00:00Z").unwrap())
            .execute(state.pool())
            .await
            .unwrap();

            let res = client
                .put(format!("{base_url}/api/measurements/{m_id}"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "status": "completed",
                    "record_as_tenko": true,
                    "tenko_type": "pre_operation",
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200, "再送の保存は成功する");

            let (sessions, records) = type_and_method(state.pool(), tenant, m_id).await;
            assert_eq!(
                sessions,
                vec![("normal".to_string(), "自動点呼".to_string())],
                "旧版の 1 行のまま (pre の 2 行目はできない)"
            );
            assert!(records.is_empty(), "記録も増えない");
        }
    );
}

#[tokio::test]
async fn test_remote_tenko_session_is_not_caught_by_normal_flow_index() {
    test_group!("通常点呼 → 点呼記録 (始業・終業)");
    test_case!(
        "スケジュール無し (遠隔点呼) の session は '通常点呼' ではないため、\
         通常点呼と同じ測定を付けても unique に当たらない (Refs ippoan/rust-alc-api#655)",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant = common::create_test_tenant(state.pool(), "Normal Tenko Auto").await;
            let jwt = common::create_test_jwt(tenant, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let emp =
                common::create_test_employee(&client, &base_url, &auth, "運行者", "NT12").await;
            let emp_id = emp["id"].as_str().unwrap();

            // 通常点呼 (終業) で記録済みの測定
            let res = send_measurement(
                &client,
                &base_url,
                &auth,
                emp_id,
                "pass",
                "2026-09-12T13:00:00Z",
                true,
                Some("post_operation"),
            )
            .await;
            assert_eq!(res.status(), 201);
            let m: Value = res.json().await.unwrap();
            let m_id = m["id"].as_str().unwrap();

            // 遠隔点呼 (スケジュール無し) の終業 (Refs ippoan/rust-alc-api#655)
            let res = client
                .post(format!("{base_url}/api/tenko/sessions/start"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "employee_id": emp_id,
                    "tenko_type": "post_operation",
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let session: Value = res.json().await.unwrap();
            let session_id = session["id"].as_str().unwrap();

            let method: String =
                sqlx::query_scalar("SELECT tenko_method FROM alc_api.tenko_sessions WHERE id = $1")
                    .bind(Uuid::parse_str(session_id).unwrap())
                    .fetch_one(state.pool())
                    .await
                    .unwrap();
            assert_eq!(method, "遠隔点呼", "スケジュール無しの session は遠隔点呼のはず (Refs ippoan/rust-alc-api#655)");

            // 同じ測定を遠隔点呼のセッションに付ける
            let res = client
                .put(format!(
                    "{base_url}/api/tenko/sessions/{session_id}/alcohol"
                ))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "measurement_id": m_id,
                    "alcohol_result": "pass",
                    "alcohol_value": 0.0,
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(
                res.status(),
                200,
                "遠隔点呼の measurement_id の書き込みは部分 unique に当たらない"
            );
        }
    );
}

// ---------------------------------------------------------------------------
// 電子車検証の番号と carins の車検期限 (migration 142、Refs ippoan/alc-app-s3#110)
//
// 値はすべて合成値。照合の SQL (alc-core の repo/car_inspections.rs) は RLS と
// 正規表現・to_date を実 DB で通すので mock では検証できない。
// ---------------------------------------------------------------------------

/// car_inspection に合成の行を 1 件入れる (取り込みと同じ UPSERT を通す)
async fn insert_car_inspection(
    state: &rust_alc_api::AppState,
    tenant: Uuid,
    cert_no: &str,
    car_id: &str,
    expirdate: &str,
    grantdate_d: &str,
) {
    let cert_info = serde_json::json!({
        "ElectCertMgNo": cert_no,
        "CarId": car_id,
        "EntryNoCarNo": format!("TEST-CAR-NO-{cert_no}"),
        "GrantdateE": "R",
        "GrantdateY": "07",
        "GrantdateM": "01",
        "GrantdateD": grantdate_d,
        "TwodimensionCodeInfoValidPeriodExpirdate": expirdate,
    });
    state
        .car_inspections
        .upsert_from_json(tenant, &cert_info, "test")
        .await
        .expect("car_inspection の合成行を入れられない");
}

/// 電子車検証の番号付きで通常点呼の測定を保存する
#[allow(clippy::too_many_arguments)]
async fn send_carins_measurement(
    client: &reqwest::Client,
    base_url: &str,
    auth: &str,
    employee_id: &str,
    measured_at: &str,
    cert_no: Option<&str>,
    vehicle_id: Option<&str>,
) -> reqwest::Response {
    let mut body = serde_json::json!({
        "employee_id": employee_id,
        "alcohol_value": 0.0,
        "result_type": "normal",
        "measured_at": measured_at,
        "record_as_tenko": true,
    });
    if let Some(v) = cert_no {
        body["carins_cert_no"] = Value::from(v);
    }
    if let Some(v) = vehicle_id {
        body["carins_vehicle_id"] = Value::from(v);
    }
    client
        .post(format!("{base_url}/api/measurements"))
        .header("Authorization", auth)
        .json(&body)
        .send()
        .await
        .unwrap()
}

type CarinsCols = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// 測定に紐づく session の 4 列と、record の record_data の同じ 4 値
async fn carins_of(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    measurement: &Value,
) -> (CarinsCols, CarinsCols) {
    let mid = Uuid::parse_str(measurement["id"].as_str().unwrap()).unwrap();
    let session: CarinsCols = sqlx::query_as(
        "SELECT carins_cert_no, carins_vehicle_id, carins_expires_on::text, carins_matched_by
         FROM alc_api.tenko_sessions WHERE tenant_id = $1 AND measurement_id = $2",
    )
    .bind(tenant_id)
    .bind(mid)
    .fetch_one(pool)
    .await
    .unwrap();
    let record: CarinsCols = sqlx::query_as(
        "SELECT r.record_data->>'carins_cert_no', r.record_data->>'carins_vehicle_id',
                r.record_data->>'carins_expires_on', r.record_data->>'carins_matched_by'
         FROM alc_api.tenko_records r
         JOIN alc_api.tenko_sessions s ON s.id = r.session_id
         WHERE r.tenant_id = $1 AND s.measurement_id = $2",
    )
    .bind(tenant_id)
    .bind(mid)
    .fetch_one(pool)
    .await
    .unwrap();
    (session, record)
}

fn cols(
    cert_no: Option<&str>,
    vehicle_id: Option<&str>,
    expires_on: Option<&str>,
    matched_by: Option<&str>,
) -> CarinsCols {
    (
        cert_no.map(str::to_string),
        vehicle_id.map(str::to_string),
        expires_on.map(str::to_string),
        matched_by.map(str::to_string),
    )
}

#[tokio::test]
async fn test_carins_expiry_is_recorded() {
    test_group!("通常点呼 → 点呼記録 (電子車検証)");
    test_case!(
        "管理番号一致 / 車両 ID 一致 / 未登録 / 番号なし を session・record_data・CSV に残す",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant = common::create_test_tenant(state.pool(), "Normal Tenko Carins").await;
            let jwt = common::create_test_jwt(tenant, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let emp =
                common::create_test_employee(&client, &base_url, &auth, "運行者", "NTC1").await;
            let emp_id = emp["id"].as_str().unwrap();
            insert_car_inspection(
                &state,
                tenant,
                "000000000001",
                "TESTCARID00001",
                "301231",
                "01",
            )
            .await;

            // 管理番号で一致
            let res = send_carins_measurement(
                &client,
                &base_url,
                &auth,
                emp_id,
                "2026-09-15T01:00:00Z",
                Some("000000000001"),
                Some("TESTCARID00009"),
            )
            .await;
            assert_eq!(res.status(), 201);
            let by_cert: Value = res.json().await.unwrap();
            let expected = cols(
                Some("000000000001"),
                Some("TESTCARID00009"),
                Some("2030-12-31"),
                Some("cert_no"),
            );
            assert_eq!(
                carins_of(state.pool(), tenant, &by_cert).await,
                (expected.clone(), expected)
            );

            // 管理番号は不一致、車両 ID で一致
            let res = send_carins_measurement(
                &client,
                &base_url,
                &auth,
                emp_id,
                "2026-09-15T02:00:00Z",
                Some("000000000009"),
                Some("TESTCARID00001"),
            )
            .await;
            assert_eq!(res.status(), 201);
            let by_car: Value = res.json().await.unwrap();
            let expected = cols(
                Some("000000000009"),
                Some("TESTCARID00001"),
                Some("2030-12-31"),
                Some("car_id"),
            );
            assert_eq!(
                carins_of(state.pool(), tenant, &by_car).await,
                (expected.clone(), expected)
            );

            // carins に無い → none・期限 NULL (点呼は記録する)
            let res = send_carins_measurement(
                &client,
                &base_url,
                &auth,
                emp_id,
                "2026-09-15T03:00:00Z",
                Some("000000000008"),
                None,
            )
            .await;
            assert_eq!(res.status(), 201);
            let none: Value = res.json().await.unwrap();
            let expected = cols(Some("000000000008"), None, None, Some("none"));
            assert_eq!(
                carins_of(state.pool(), tenant, &none).await,
                (expected.clone(), expected)
            );

            // 番号を送らない (空文字も無しと同じ) → 4 列とも NULL
            let res = send_carins_measurement(
                &client,
                &base_url,
                &auth,
                emp_id,
                "2026-09-15T04:00:00Z",
                Some(""),
                None,
            )
            .await;
            assert_eq!(res.status(), 201);
            let without: Value = res.json().await.unwrap();
            let expected = cols(None, None, None, None);
            assert_eq!(
                carins_of(state.pool(), tenant, &without).await,
                (expected.clone(), expected)
            );
            assert_eq!(counts(state.pool(), tenant).await, (4, 4));

            // CSV の末尾 4 列
            let res = client
                .get(format!("{base_url}/api/tenko/records/csv"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let csv = res.text().await.unwrap();
            let mut lines = csv.trim_start_matches('\u{feff}').lines();
            let header = lines.next().unwrap();
            assert!(
                header.ends_with(
                    "record_hash,carins_cert_no,carins_vehicle_id,carins_expires_on,carins_matched_by"
                ),
                "CSV の末尾 4 列: {header}"
            );
            let tails: Vec<String> = lines
                .map(|l| l.rsplitn(5, ',').collect::<Vec<_>>()[..4].join("|"))
                .collect();
            for expected in [
                "cert_no|2030-12-31|TESTCARID00009|000000000001",
                "car_id|2030-12-31|TESTCARID00001|000000000009",
                "none|||000000000008",
                "|||",
            ] {
                assert!(
                    tails.iter().any(|t| t == expected),
                    "CSV に {expected} の行がある: {tails:?}"
                );
            }
        }
    );
}

#[tokio::test]
async fn test_carins_newer_row_of_same_car_wins() {
    test_group!("通常点呼 → 点呼記録 (電子車検証)");
    test_case!(
        "同じ車両 ID で管理番号の違う古い行と新しい行 → 新しい期限 (記録と照合口の両方)",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant = common::create_test_tenant(state.pool(), "Normal Tenko Carins New").await;
            let jwt = common::create_test_jwt(tenant, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let emp =
                common::create_test_employee(&client, &base_url, &auth, "運行者", "NTC2").await;
            let emp_id = emp["id"].as_str().unwrap();
            // 継続検査の前 (古い管理番号) と後 (新しい管理番号)
            insert_car_inspection(
                &state,
                tenant,
                "000000000002",
                "TESTCARID00002",
                "250101",
                "01",
            )
            .await;
            insert_car_inspection(
                &state,
                tenant,
                "000000000003",
                "TESTCARID00002",
                "301231",
                "02",
            )
            .await;

            // 古い電子車検証の番号でタップしても、同じ車の新しい期限が出る
            let res = send_carins_measurement(
                &client,
                &base_url,
                &auth,
                emp_id,
                "2026-09-15T05:00:00Z",
                Some("000000000002"),
                Some("TESTCARID00002"),
            )
            .await;
            assert_eq!(res.status(), 201);
            let m: Value = res.json().await.unwrap();
            let (session, _) = carins_of(state.pool(), tenant, &m).await;
            assert_eq!(session.2.as_deref(), Some("2030-12-31"));
            assert_eq!(
                session.3.as_deref(),
                Some("car_id"),
                "新しい行は管理番号が違うので car_id で一致"
            );

            // 照合口 (kiosk) も同じ SQL
            let res = client
                .post(format!("{base_url}/api/car-inspections/lookup"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({"cert_no": "000000000003", "car_id": "TESTCARID00002"}))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert_eq!(
                body,
                serde_json::json!({
                    "expires_on": "2030-12-31",
                    "matched_by": "cert_no",
                    "car_no": "TEST-CAR-NO-000000000003",
                })
            );
        }
    );
}

#[tokio::test]
async fn test_carins_broken_expiry_still_records() {
    test_group!("通常点呼 → 点呼記録 (電子車検証)");
    test_case!(
        "壊れた期限の行でも記録は作られ、期限は NULL",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant =
                common::create_test_tenant(state.pool(), "Normal Tenko Carins Broken").await;
            let jwt = common::create_test_jwt(tenant, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let emp =
                common::create_test_employee(&client, &base_url, &auth, "運行者", "NTC3").await;
            let emp_id = emp["id"].as_str().unwrap();
            // 月が 13 — 正規表現で外れて NULL (照合は成功)
            insert_car_inspection(
                &state,
                tenant,
                "000000000004",
                "TESTCARID00004",
                "301399",
                "01",
            )
            .await;
            // 2 月 31 日 — 正規表現は通るが to_date が失敗する (照合の SQL エラー)
            insert_car_inspection(
                &state,
                tenant,
                "000000000005",
                "TESTCARID00005",
                "300231",
                "01",
            )
            .await;

            let res = send_carins_measurement(
                &client,
                &base_url,
                &auth,
                emp_id,
                "2026-09-15T06:00:00Z",
                Some("000000000004"),
                None,
            )
            .await;
            assert_eq!(res.status(), 201);
            let m: Value = res.json().await.unwrap();
            let expected = cols(Some("000000000004"), None, None, Some("cert_no"));
            assert_eq!(
                carins_of(state.pool(), tenant, &m).await,
                (expected.clone(), expected)
            );

            let res = send_carins_measurement(
                &client,
                &base_url,
                &auth,
                emp_id,
                "2026-09-15T07:00:00Z",
                Some("000000000005"),
                Some("TESTCARID00005"),
            )
            .await;
            assert_eq!(res.status(), 201);
            let m: Value = res.json().await.unwrap();
            let expected = cols(Some("000000000005"), Some("TESTCARID00005"), None, None);
            assert_eq!(
                carins_of(state.pool(), tenant, &m).await,
                (expected.clone(), expected),
                "照合に失敗しても番号は保存し、期限と matched_by は NULL"
            );
            assert_eq!(counts(state.pool(), tenant).await, (2, 2));
        }
    );
}

#[tokio::test]
async fn test_invalid_carins_number_is_rejected() {
    test_group!("通常点呼 → 点呼記録 (電子車検証)");
    test_case!(
        "不正な桁の番号は 400 で、測定も記録も作らない (POST / PUT)",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant =
                common::create_test_tenant(state.pool(), "Normal Tenko Carins Invalid").await;
            let jwt = common::create_test_jwt(tenant, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let emp =
                common::create_test_employee(&client, &base_url, &auth, "運行者", "NTC4").await;
            let emp_id = emp["id"].as_str().unwrap();

            // POST
            let res = send_carins_measurement(
                &client,
                &base_url,
                &auth,
                emp_id,
                "2026-09-15T08:00:00Z",
                Some("0000000001"),
                None,
            )
            .await;
            assert_eq!(res.status(), 400);
            assert_eq!(measurement_count(state.pool(), tenant).await, 0);

            // PUT (測定開始 → 完了 PUT に不正な車両 ID)
            let res = client
                .post(format!("{base_url}/api/measurements/start"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "employee_id": emp_id }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let started: Value = res.json().await.unwrap();
            let m_id = started["id"].as_str().unwrap();

            let res = client
                .put(format!("{base_url}/api/measurements/{m_id}"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "status": "completed",
                    "alcohol_value": 0.0,
                    "result_type": "normal",
                    "measured_at": "2026-09-15T08:30:00Z",
                    "record_as_tenko": true,
                    "carins_vehicle_id": "TESTCARID-0001",
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 400);

            let status: String =
                sqlx::query_scalar("SELECT status FROM alc_api.measurements WHERE id = $1")
                    .bind(Uuid::parse_str(m_id).unwrap())
                    .fetch_one(state.pool())
                    .await
                    .unwrap();
            assert_eq!(status, "started", "400 の PUT は測定を更新しない");
            assert_eq!(counts(state.pool(), tenant).await, (0, 0));
        }
    );
}

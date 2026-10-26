//! 通常の流れ (通常点呼 / IT点呼) の点呼の行を、rust に残る点呼の口 (一覧・判定・CSV) が
//! 正しく扱うことを実 DB で固定する。
//!
//! Refs ippoan/alc-app#238, ippoan/alc-app-s3#135, ippoan/alc-app#387
//!
//! 行を作る側 (測定の保存 → 点呼セッション・点呼記録) は alc-measurements-worker へ移った
//! (Refs #680)。その冪等・車検証・本人確認の方法などは worker の `normal_tenko_db.rs` が持つ。
//! ここでは worker が書くのと同じ形の行を `common::insert_normal_flow_tenko` で直接入れ、
//! rust の口の振る舞いだけを見る。
//!
//! **実 DB でしか検証できない** — 部分 unique (migration 141) と、執行者 NULL の記録を
//! 一覧・CSV が壊れずに返せることは、repository を差し替える mock テストでは通らない。
//!
//! 回し方は他の DB integration テストと同じ:
//!   `make db-up && source .test-config && cargo test --test normal_tenko_record_test`
//! CI は ci.yml の `bazel-test-db` shard (postgres service + TEST_DATABASE_URL)。

#[macro_use]
mod common;

use serde_json::Value;
use uuid::Uuid;

/// 乗務員を作り、id を返す
async fn employee(pool: &sqlx::PgPool, tenant: Uuid, name: &str, code: &str) -> Uuid {
    let emp = common::create_test_employee(pool, tenant, name, code).await;
    Uuid::parse_str(emp["id"].as_str().unwrap()).unwrap()
}

#[tokio::test]
async fn test_normal_tenko_record_is_listed_with_empty_manager() {
    test_group!("通常点呼の記録 → 一覧・CSV");
    test_case!(
        "執行者が空欄の通常点呼の記録が、記録の一覧・CSV・セッションの一覧に出る",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant = common::create_test_tenant(state.pool(), "Normal Tenko Create").await;
            let jwt = common::create_test_jwt(tenant, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let emp = employee(state.pool(), tenant, "運行者", "NT1").await;
            common::insert_normal_flow_tenko(
                state.pool(),
                tenant,
                emp,
                "2026-09-12T01:00:00Z",
                "normal",
                "通常点呼",
                false,
            )
            .await;

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

            let emp = common::create_test_employee(state.pool(), tenant, "運行者", "NT12").await;
            let emp_id = emp["id"].as_str().unwrap();

            // 通常点呼 (終業) で記録済みの測定
            let (m_id, _) = common::insert_normal_flow_tenko(
                state.pool(),
                tenant,
                Uuid::parse_str(emp_id).unwrap(),
                "2026-09-12T13:00:00Z",
                "post_operation",
                "通常点呼",
                false,
            )
            .await;
            let m_id = m_id.to_string();

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
            assert_eq!(
                method, "遠隔点呼",
                "スケジュール無しの session は遠隔点呼のはず (Refs ippoan/rust-alc-api#655)"
            );

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

#[tokio::test]
async fn test_it_tenko_session_accepts_manager_judgment() {
    test_group!("通常点呼 → 点呼記録 (点呼方法)");
    test_case!(
        "IT点呼 の session に運行管理者の判定が入り、GET に点呼方法と判定が出る",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant = common::create_test_tenant(state.pool(), "Tenko Method Judgment").await;
            let jwt = common::create_test_jwt(tenant, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let emp = common::create_test_employee(state.pool(), tenant, "運行者", "NT35").await;
            let emp_id = emp["id"].as_str().unwrap();

            // 判定する運行管理者 (employees.role に manager を持つ)
            let manager = common::create_test_employee_with_role(
                state.pool(),
                tenant,
                "運行管理者",
                "NT35M",
                &["manager"],
            )
            .await;
            let manager_id = manager["id"].as_str().unwrap();

            let (m_id, session_id) = common::insert_normal_flow_tenko(
                state.pool(),
                tenant,
                Uuid::parse_str(emp_id).unwrap(),
                "2026-09-13T08:00:00Z",
                "pre_operation",
                "IT点呼",
                false,
            )
            .await;
            let (m_id, session_id) = (m_id.to_string(), session_id.to_string());

            // 保存の直後: 完了扱いで、判定はまだ無い (= 判定待ち)
            let res = client
                .get(format!("{base_url}/api/tenko/sessions/{session_id}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let session: Value = res.json().await.unwrap();
            assert_eq!(session["tenko_method"], "IT点呼");
            assert_eq!(session["status"], "completed");
            assert_eq!(session["measurement_id"], m_id);
            assert!(session["manager_judgment"].is_null());

            // 運行管理者の判定 (管理者ログインの経路)
            let res = client
                .post(format!(
                    "{base_url}/api/tenko/sessions/{session_id}/judgment"
                ))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "judgment": "ok",
                    "judged_by_employee_id": manager_id,
                    // IT点呼 は確認の方法を選んで確定する (alc-migrations 157)
                    "method": "it",
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200, "IT点呼 の session でも判定を受ける");

            let res = client
                .get(format!("{base_url}/api/tenko/sessions/{session_id}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let session: Value = res.json().await.unwrap();
            assert_eq!(session["tenko_method"], "IT点呼");
            assert_eq!(session["manager_judgment"], "ok");
            assert_eq!(session["manager_judgment_by"], manager_id);
            assert_eq!(session["status"], "completed", "判定は status を変えない");
        }
    );
}

// =========================================================================
// IT点呼 の判定の確認の方法 (通話 / 対面)、未完了の IT点呼 の一覧、CSV の読み替え
// (alc-migrations 157、Refs ippoan/alc-app#387)
// =========================================================================

/// 判定する運行管理者 (employees.role に manager を持つ) を作り、id を返す
async fn create_manager(pool: &sqlx::PgPool, tenant: Uuid, code: &str) -> String {
    let manager =
        common::create_test_employee_with_role(pool, tenant, "運行管理者", code, &["manager"])
            .await;
    manager["id"].as_str().unwrap().to_string()
}

/// 始業の点呼のセッション (と記録) を 1 件入れ、session の id を返す
/// (`tenko_method` が None なら通常点呼)
async fn tenko_session(
    pool: &sqlx::PgPool,
    tenant: Uuid,
    employee_id: &str,
    measured_at: &str,
    tenko_method: Option<&str>,
) -> String {
    let (_, session_id) = common::insert_normal_flow_tenko(
        pool,
        tenant,
        Uuid::parse_str(employee_id).unwrap(),
        measured_at,
        "pre_operation",
        tenko_method.unwrap_or("通常点呼"),
        false,
    )
    .await;
    session_id.to_string()
}

/// 判定の POST を送り、応答をそのまま返す (`method` は付けたときだけ載る)
async fn post_judgment(
    client: &reqwest::Client,
    base_url: &str,
    auth: &str,
    session_id: &str,
    judgment: &str,
    manager_id: &str,
    method: Option<&str>,
) -> reqwest::Response {
    let mut body = serde_json::json!({
        "judgment": judgment,
        "judged_by_employee_id": manager_id,
    });
    if let Some(method) = method {
        body["method"] = Value::from(method);
    }
    client
        .post(format!(
            "{base_url}/api/tenko/sessions/{session_id}/judgment"
        ))
        .header("Authorization", auth)
        .json(&body)
        .send()
        .await
        .unwrap()
}

/// 行の (manager_judgment, manager_judgment_method) を DB から直接引く
async fn judgment_columns(
    pool: &sqlx::PgPool,
    session_id: &str,
) -> (Option<String>, Option<String>) {
    sqlx::query_as(
        "SELECT manager_judgment, manager_judgment_method
         FROM alc_api.tenko_sessions WHERE id = $1",
    )
    .bind(Uuid::parse_str(session_id).unwrap())
    .fetch_one(pool)
    .await
    .unwrap()
}

/// 点呼セッションの一覧を query 付きで引き、(total, 並べ替えた id) を返す
async fn list_sessions(
    client: &reqwest::Client,
    base_url: &str,
    auth: &str,
    query: &[(&str, &str)],
) -> (i64, Vec<String>) {
    let res = client
        .get(format!("{base_url}/api/tenko/sessions"))
        .header("Authorization", auth)
        .query(query)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "一覧 {query:?}");
    let body: Value = res.json().await.unwrap();
    let mut ids: Vec<String> = body["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].as_str().unwrap().to_string())
        .collect();
    ids.sort();
    (body["total"].as_i64().unwrap(), ids)
}

fn sorted(ids: &[&String]) -> Vec<String> {
    let mut v: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
    v.sort();
    v
}

#[tokio::test]
async fn test_it_tenko_judgment_method_is_stored_and_kept() {
    test_group!("IT点呼 の判定の確認の方法");
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant = common::create_test_tenant(state.pool(), "IT Judgment Method").await;
    let jwt = common::create_test_jwt(tenant, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(state.pool(), tenant, "運行者", "NT41").await;
    let emp_id = emp["id"].as_str().unwrap();
    let manager_id = create_manager(state.pool(), tenant, "NT41M").await;

    let s_in_person = tenko_session(
        state.pool(),
        tenant,
        emp_id,
        "2026-09-14T08:00:00Z",
        Some("IT点呼"),
    )
    .await;
    let s_it = tenko_session(
        state.pool(),
        tenant,
        emp_id,
        "2026-09-14T09:00:00Z",
        Some("IT点呼"),
    )
    .await;
    let s_normal = tenko_session(state.pool(), tenant, emp_id, "2026-09-14T10:00:00Z", None).await;

    test_case!(
        "IT点呼 の最初の確定に method が無ければ 400 で、行は変わらない",
        {
            let res = post_judgment(
                &client,
                &base_url,
                &auth,
                &s_in_person,
                "ok",
                &manager_id,
                None,
            )
            .await;
            assert_eq!(res.status(), 400);
            assert_eq!(
                judgment_columns(state.pool(), &s_in_person).await,
                (None, None)
            );
        }
    );

    test_case!("不正な method は 400 で、行は変わらない", {
        let res = post_judgment(
            &client,
            &base_url,
            &auth,
            &s_in_person,
            "ok",
            &manager_id,
            Some("phone"),
        )
        .await;
        assert_eq!(res.status(), 400);
        assert_eq!(
            judgment_columns(state.pool(), &s_in_person).await,
            (None, None)
        );
    });

    test_case!("in_person / it が列に入り、GET の JSON に出る", {
        for (session_id, method) in [(&s_in_person, "in_person"), (&s_it, "it")] {
            let res = post_judgment(
                &client,
                &base_url,
                &auth,
                session_id,
                "ok",
                &manager_id,
                Some(method),
            )
            .await;
            assert_eq!(res.status(), 200, "method={method}");
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["manager_judgment"], "ok");
            assert_eq!(body["manager_judgment_method"], method);
            assert_eq!(body["tenko_method"], "IT点呼", "点呼方法は書き換えない");
            assert_eq!(
                judgment_columns(state.pool(), session_id).await,
                (Some("ok".to_string()), Some(method.to_string()))
            );

            let res = client
                .get(format!("{base_url}/api/tenko/sessions/{session_id}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let session: Value = res.json().await.unwrap();
            assert_eq!(session["manager_judgment_method"], method);
            assert_eq!(session["tenko_method"], "IT点呼");
            assert_eq!(session["status"], "completed", "判定は status を変えない");
        }
    });

    test_case!(
        "method を省いた押し直しは judgment だけ変え、確認の方法を保つ",
        {
            let res = post_judgment(
                &client,
                &base_url,
                &auth,
                &s_in_person,
                "ng",
                &manager_id,
                None,
            )
            .await;
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["manager_judgment"], "ng");
            assert_eq!(body["manager_judgment_method"], "in_person");
            assert_eq!(
                judgment_columns(state.pool(), &s_in_person).await,
                (Some("ng".to_string()), Some("in_person".to_string()))
            );
            // もう片方の行は巻き込まれない
            assert_eq!(
                judgment_columns(state.pool(), &s_it).await,
                (Some("ok".to_string()), Some("it".to_string()))
            );
        }
    );

    test_case!(
        "method を付けた押し直しはその値で上書きする",
        {
            let res = post_judgment(
                &client,
                &base_url,
                &auth,
                &s_it,
                "ok",
                &manager_id,
                Some("in_person"),
            )
            .await;
            assert_eq!(res.status(), 200);
            assert_eq!(
                judgment_columns(state.pool(), &s_it).await,
                (Some("ok".to_string()), Some("in_person".to_string()))
            );
        }
    );

    test_case!(
        "通常点呼に method を付けると 400、付けなければ今までどおりで列は NULL",
        {
            let res = post_judgment(
                &client,
                &base_url,
                &auth,
                &s_normal,
                "ok",
                &manager_id,
                Some("it"),
            )
            .await;
            assert_eq!(res.status(), 400);
            assert_eq!(
                judgment_columns(state.pool(), &s_normal).await,
                (None, None)
            );

            let res = post_judgment(
                &client,
                &base_url,
                &auth,
                &s_normal,
                "ok",
                &manager_id,
                None,
            )
            .await;
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["manager_judgment"], "ok");
            assert!(body["manager_judgment_method"].is_null());
            assert_eq!(
                judgment_columns(state.pool(), &s_normal).await,
                (Some("ok".to_string()), None)
            );
        }
    );
}

#[tokio::test]
async fn test_list_pending_it_tenko_sessions() {
    test_group!("未完了の IT点呼 の一覧");
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant = common::create_test_tenant(state.pool(), "IT Pending List").await;
    let jwt = common::create_test_jwt(tenant, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(state.pool(), tenant, "運行者", "NT42").await;
    let emp_id = emp["id"].as_str().unwrap();
    let manager_id = create_manager(state.pool(), tenant, "NT42M").await;

    // IT点呼 3 件 (a: 未判定のまま / b: 後で判定する / c: 先に判定済み) と通常点呼 1 件 (未判定)
    let it_a = tenko_session(
        state.pool(),
        tenant,
        emp_id,
        "2026-09-15T08:00:00Z",
        Some("IT点呼"),
    )
    .await;
    let it_b = tenko_session(
        state.pool(),
        tenant,
        emp_id,
        "2026-09-15T09:00:00Z",
        Some("IT点呼"),
    )
    .await;
    let it_c = tenko_session(
        state.pool(),
        tenant,
        emp_id,
        "2026-09-15T10:00:00Z",
        Some("IT点呼"),
    )
    .await;
    let normal = tenko_session(state.pool(), tenant, emp_id, "2026-09-15T11:00:00Z", None).await;
    let res = post_judgment(
        &client,
        &base_url,
        &auth,
        &it_c,
        "ok",
        &manager_id,
        Some("it"),
    )
    .await;
    assert_eq!(res.status(), 200);

    let pending_it = [("tenko_method", "IT点呼"), ("judgment_pending", "true")];

    test_case!("未判定の IT点呼 だけを返し、total も合う", {
        assert_eq!(
            list_sessions(&client, &base_url, &auth, &pending_it).await,
            (2, sorted(&[&it_a, &it_b])),
            "通常点呼と判定済みは入らない"
        );
    });

    test_case!("filter なし・片方だけの filter", {
        assert_eq!(
            list_sessions(&client, &base_url, &auth, &[]).await,
            (4, sorted(&[&it_a, &it_b, &it_c, &normal])),
            "filter なしは今までどおり全部"
        );
        assert_eq!(
            list_sessions(&client, &base_url, &auth, &[("tenko_method", "IT点呼")]).await,
            (3, sorted(&[&it_a, &it_b, &it_c]))
        );
        assert_eq!(
            list_sessions(&client, &base_url, &auth, &[("tenko_method", "通常点呼")]).await,
            (1, sorted(&[&normal]))
        );
        assert_eq!(
            list_sessions(&client, &base_url, &auth, &[("judgment_pending", "true")]).await,
            (3, sorted(&[&it_a, &it_b, &normal])),
            "未判定は点呼方法を問わない"
        );
        assert_eq!(
            list_sessions(&client, &base_url, &auth, &[("judgment_pending", "false")]).await,
            (4, sorted(&[&it_a, &it_b, &it_c, &normal])),
            "false は絞らない"
        );
    });

    test_case!(
        "ほかの filter・ページングと組み合わせても bind がずれない",
        {
            // 新しい条件の前 (employee_id / status / tenko_type) と後 (date_from / date_to) の
            // 両側に bind を置き、COUNT と本体の両方を見る
            let all = [
                ("employee_id", emp_id),
                ("status", "completed"),
                ("tenko_type", "pre_operation"),
                ("tenko_method", "IT点呼"),
                ("judgment_pending", "true"),
                ("date_from", "2026-09-15T08:30:00Z"),
                ("date_to", "2026-09-15T23:00:00Z"),
            ];
            assert_eq!(
                list_sessions(&client, &base_url, &auth, &all).await,
                (1, sorted(&[&it_b])),
                "date_from で a が外れ、b だけ残る"
            );

            let mut paged = pending_it.to_vec();
            paged.push(("per_page", "1"));
            let (total, first) = list_sessions(&client, &base_url, &auth, &paged).await;
            paged.push(("page", "2"));
            let (total2, second) = list_sessions(&client, &base_url, &auth, &paged).await;
            assert_eq!((total, total2), (2, 2), "total はページに依らない");
            assert_eq!((first.len(), second.len()), (1, 1));
            let mut both = [first, second].concat();
            both.sort();
            assert_eq!(both, sorted(&[&it_a, &it_b]));
        }
    );

    test_case!("判定後は返さない", {
        let res = post_judgment(
            &client,
            &base_url,
            &auth,
            &it_b,
            "ng",
            &manager_id,
            Some("in_person"),
        )
        .await;
        assert_eq!(res.status(), 200);
        assert_eq!(
            list_sessions(&client, &base_url, &auth, &pending_it).await,
            (1, sorted(&[&it_a]))
        );
        assert_eq!(
            list_sessions(&client, &base_url, &auth, &[]).await.0,
            4,
            "filter なしの件数は変わらない"
        );
    });
}

#[tokio::test]
async fn test_csv_reads_in_person_it_tenko_as_in_person_tenko() {
    test_group!("記録簿の CSV: 対面で確定した IT点呼");
    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let tenant = common::create_test_tenant(state.pool(), "IT CSV In Person").await;
    let jwt = common::create_test_jwt(tenant, "admin");
    let auth = format!("Bearer {jwt}");
    let client = reqwest::Client::new();

    let emp = common::create_test_employee(state.pool(), tenant, "運行者", "NT43").await;
    let emp_id = emp["id"].as_str().unwrap();
    let manager_id = create_manager(state.pool(), tenant, "NT43M").await;

    let s_in_person = tenko_session(
        state.pool(),
        tenant,
        emp_id,
        "2026-09-16T08:00:00Z",
        Some("IT点呼"),
    )
    .await;
    let s_it = tenko_session(
        state.pool(),
        tenant,
        emp_id,
        "2026-09-16T09:00:00Z",
        Some("IT点呼"),
    )
    .await;
    let s_pending = tenko_session(
        state.pool(),
        tenant,
        emp_id,
        "2026-09-16T10:00:00Z",
        Some("IT点呼"),
    )
    .await;
    let s_normal = tenko_session(state.pool(), tenant, emp_id, "2026-09-16T11:00:00Z", None).await;
    for (session_id, method) in [(&s_in_person, "in_person"), (&s_it, "it")] {
        let res = post_judgment(
            &client,
            &base_url,
            &auth,
            session_id,
            "ok",
            &manager_id,
            Some(method),
        )
        .await;
        assert_eq!(res.status(), 200);
    }

    // 記録の id → (session の id, DB の tenko_method)
    let records: Vec<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT id, session_id, tenko_method FROM alc_api.tenko_records WHERE tenant_id = $1",
    )
    .bind(tenant)
    .fetch_all(state.pool())
    .await
    .unwrap();
    assert_eq!(records.len(), 4);
    let record_of = |session_id: &String| -> String {
        let sid = Uuid::parse_str(session_id).unwrap();
        records
            .iter()
            .find(|(_, s, _)| *s == sid)
            .map(|(id, _, _)| id.to_string())
            .unwrap()
    };

    test_case!(
        "対面で確定した行だけ対面の点呼として出る。列は増えない",
        {
            let res = client
                .get(format!("{base_url}/api/tenko/records/csv"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let csv = res.text().await.unwrap();
            let csv = csv.trim_start_matches('\u{feff}');
            let mut lines = csv.lines();
            let header: Vec<&str> = lines.next().unwrap().split(',').collect();
            assert!(!header.contains(&"manager_judgment_method"));
            assert_eq!(header.len(), 39, "列の数は変わらない");
            assert_eq!(header[3], "tenko_method", "列の位置も変わらない");
            // 記録の id (先頭の列) → tenko_method (4 列目。その手前の値にカンマは無い)
            let by_record: std::collections::HashMap<String, String> = lines
                .map(|line| {
                    let cols: Vec<&str> = line.split(',').collect();
                    (cols[0].to_string(), cols[3].to_string())
                })
                .collect();
            assert_eq!(by_record.len(), 4);
            assert_eq!(by_record[&record_of(&s_in_person)], "対面点呼");
            assert_eq!(by_record[&record_of(&s_it)], "IT点呼");
            assert_eq!(by_record[&record_of(&s_pending)], "IT点呼");
            assert_eq!(by_record[&record_of(&s_normal)], "通常点呼");
        }
    );

    test_case!(
        "tenko_records の行と JSON の応答は読み替えない",
        {
            let normal_sid = Uuid::parse_str(&s_normal).unwrap();
            for (_, session_id, tenko_method) in &records {
                let expected = if *session_id == normal_sid {
                    "通常点呼"
                } else {
                    "IT点呼"
                };
                assert_eq!(tenko_method, expected, "記録の行そのものは書き換わらない");
            }
            // 判定の後に引き直しても同じ (記録は不変)
            let it_rows: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM alc_api.tenko_records
             WHERE tenant_id = $1 AND tenko_method = 'IT点呼'",
            )
            .bind(tenant)
            .fetch_one(state.pool())
            .await
            .unwrap();
            assert_eq!(it_rows, 3);

            let res = client
                .get(format!("{base_url}/api/tenko/records"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["total"], 4);
            let in_person_record = record_of(&s_in_person);
            let row = body["records"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"] == in_person_record.as_str())
                .unwrap();
            assert_eq!(row["tenko_method"], "IT点呼", "JSON は読み替えない");

            let res = client
                .get(format!("{base_url}/api/tenko/records/{in_person_record}"))
                .header("Authorization", &auth)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let one: Value = res.json().await.unwrap();
            assert_eq!(one["tenko_method"], "IT点呼");
        }
    );
}

//! 自動点呼タブで血圧 (最高・最低) を必須にし、測れないときは遠隔点呼へ切り替えられる
//! ことを実 DB で固定する。
//!
//! Refs ippoan/alc-app-s3#135
//!
//! **実 DB でしか検証できない** — migration 144 の CHECK 制約 (tenko_method に
//! '遠隔点呼' を追加) と escalate-remote の UPDATE は、repository を丸ごと
//! 差し替える mock テストでは SQL を 1 行も通らない。
//!
//! 回し方は他の DB integration テストと同じ:
//!   `make db-up && source .test-config && cargo test --test tenko_bp_required_escalate_test`
//! CI は ci.yml の `bazel-test-db` shard (postgres service + TEST_DATABASE_URL)。

#[macro_use]
mod common;

use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

/// 自動点呼 (キオスク) の業務前セッションを開始する。
///
/// スケジュール無しで開始すると #655 の修正により tenko_method は既定で '遠隔点呼' に
/// なる (このファイルの `no_schedule_start_session_sets_remote_method` が固定している)。
/// このファイルの他のテストは自動点呼固有の挙動 (血圧必須・escalate-remote) を確かめる
/// ものなので、作成直後に明示的に '自動点呼' へ上書きする。
async fn start_pre_operation_session(
    client: &reqwest::Client,
    base_url: &str,
    auth: &str,
    employee_id: &str,
    pool: &PgPool,
) -> Value {
    let res = client
        .post(format!("{base_url}/api/tenko/sessions/start"))
        .header("Authorization", auth)
        .json(&serde_json::json!({
            "employee_id": employee_id,
            "tenko_type": "pre_operation",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201, "session start failed");
    let mut body: Value = res.json().await.unwrap();
    assert_eq!(body["status"], "medical_pending");

    let session_id = Uuid::parse_str(body["id"].as_str().unwrap()).unwrap();
    sqlx::query("UPDATE alc_api.tenko_sessions SET tenko_method = '自動点呼' WHERE id = $1")
        .bind(session_id)
        .execute(pool)
        .await
        .unwrap();
    body["tenko_method"] = Value::from("自動点呼");
    body
}

async fn put_medical(
    client: &reqwest::Client,
    base_url: &str,
    auth: &str,
    session_id: &str,
    with_bp: bool,
) -> reqwest::Response {
    let mut body = serde_json::json!({ "temperature": 36.5 });
    if with_bp {
        body["systolic"] = Value::from(120);
        body["diastolic"] = Value::from(80);
    }
    client
        .put(format!(
            "{base_url}/api/tenko/sessions/{session_id}/medical"
        ))
        .header("Authorization", auth)
        .json(&body)
        .send()
        .await
        .unwrap()
}

async fn record_count(pool: &PgPool, session_id: Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM alc_api.tenko_records WHERE session_id = $1")
        .bind(session_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn auto_tenko_requires_bp() {
    test_group!("自動点呼タブの血圧必須化 (Refs ippoan/alc-app-s3#135)");

    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let client = reqwest::Client::new();

    let tenant = common::create_test_tenant(state.pool(), "BP Required Test").await;
    let jwt = common::create_test_jwt(tenant, "admin");
    let auth = format!("Bearer {jwt}");

    let employee =
        common::create_test_employee(&client, &base_url, &auth, "BP Test Driver", "BP001").await;
    let employee_id = employee["id"].as_str().unwrap();

    test_case!("自動点呼で血圧なしは 400 で弾く", {
        let session =
            start_pre_operation_session(&client, &base_url, &auth, employee_id, state.pool()).await;
        let session_id = session["id"].as_str().unwrap();

        let res = put_medical(&client, &base_url, &auth, session_id, false).await;
        assert_eq!(res.status(), 400, "自動点呼は血圧なしを弾くはず");
    });

    test_case!("自動点呼で血圧ありは 200", {
        let session =
            start_pre_operation_session(&client, &base_url, &auth, employee_id, state.pool()).await;
        let session_id = session["id"].as_str().unwrap();

        let res = put_medical(&client, &base_url, &auth, session_id, true).await;
        assert_eq!(res.status(), 200, "血圧ありは通るはず");
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["status"], "self_declaration_pending");
    });

    test_case!(
        "通常点呼は血圧なしでも 200 (退行がないこと)",
        {
            // 通常点呼は normal_tenko::record 経由だと即 completed で作られてしまい
            // medical_pending を経由できないため、tenko_method だけ直接書き換えて
            // submit_medical のバリデーション分岐を単独で確かめる
            let session =
                start_pre_operation_session(&client, &base_url, &auth, employee_id, state.pool())
                    .await;
            let session_id = session["id"].as_str().unwrap();
            let session_uuid = Uuid::parse_str(session_id).unwrap();

            sqlx::query(
                "UPDATE alc_api.tenko_sessions SET tenko_method = '通常点呼' WHERE id = $1",
            )
            .bind(session_uuid)
            .execute(state.pool())
            .await
            .unwrap();

            let res = put_medical(&client, &base_url, &auth, session_id, false).await;
            assert_eq!(
                res.status(),
                200,
                "通常点呼は血圧なしでも従来どおり通るはず (現場が止まってはいけない)"
            );
        }
    );
}

#[tokio::test]
async fn escalate_to_remote_flow() {
    test_group!("遠隔点呼への切り替え (Refs ippoan/alc-app-s3#135)");

    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let client = reqwest::Client::new();

    let tenant = common::create_test_tenant(state.pool(), "Escalate Remote Test").await;
    let jwt = common::create_test_jwt(tenant, "admin");
    let auth = format!("Bearer {jwt}");

    let employee =
        common::create_test_employee(&client, &base_url, &auth, "Escalate Driver", "ESC001").await;
    let employee_id = employee["id"].as_str().unwrap();

    test_case!(
        "遠隔への切り替えで tenko_method が変わり、時刻と理由が残る / 血圧なしでも 200 / 記録は 1 件のまま",
        {
            let session =
                start_pre_operation_session(&client, &base_url, &auth, employee_id, state.pool()).await;
            let session_id = session["id"].as_str().unwrap();
            let session_uuid = Uuid::parse_str(session_id).unwrap();

            // 切り替え前: 記録はまだ無い
            assert_eq!(record_count(state.pool(), session_uuid).await, 0);

            let res = client
                .put(format!(
                    "{base_url}/api/tenko/sessions/{session_id}/escalate-remote"
                ))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "reason": "血圧計が壊れている" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["tenko_method"], "遠隔点呼");
            assert_eq!(body["remote_escalation_reason"], "血圧計が壊れている");
            // JSON キーは escalated_to_remote_at (親の決定 — 画面側の管理者バッジがこの名前を見る)
            assert!(
                body["escalated_to_remote_at"].is_string(),
                "切り替え時刻が残っているはず (応答: {body})"
            );
            // status は変えない (血圧を必須にしないまま医療データ提出へ進める)
            assert_eq!(body["status"], "medical_pending");

            // DB 側にも直接残っていること
            let (db_method, db_reason, db_escalated_at): (
                String,
                Option<String>,
                Option<chrono::DateTime<chrono::Utc>>,
            ) = sqlx::query_as(
                "SELECT tenko_method, remote_escalation_reason, escalated_to_remote_at
                 FROM alc_api.tenko_sessions WHERE id = $1",
            )
            .bind(session_uuid)
            .fetch_one(state.pool())
            .await
            .unwrap();
            assert_eq!(db_method, "遠隔点呼");
            assert_eq!(db_reason.as_deref(), Some("血圧計が壊れている"));
            assert!(db_escalated_at.is_some());

            // 切り替えた点呼は血圧なしでも 200
            let res = put_medical(&client, &base_url, &auth, session_id, false).await;
            assert_eq!(res.status(), 200, "遠隔点呼へ切り替え後は血圧なしでも通るはず");

            // ここまでは記録を 1 件も作らない (record は完了 / 中止で初めて作られる)
            assert_eq!(record_count(state.pool(), session_uuid).await, 0);

            // 完了させて記録が 1 件だけ作られること、かつ「自動点呼」に化けていないことを確認
            let res = client
                .post(format!("{base_url}/api/tenko/sessions/{session_id}/cancel"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "reason": "テスト終了のため中止" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);

            assert_eq!(
                record_count(state.pool(), session_uuid).await,
                1,
                "切り替えても点呼の記録は 1 件のまま (二重にならない)"
            );
            let record_method: String = sqlx::query_scalar(
                "SELECT tenko_method FROM alc_api.tenko_records WHERE session_id = $1",
            )
            .bind(session_uuid)
            .fetch_one(state.pool())
            .await
            .unwrap();
            assert_eq!(
                record_method, "遠隔点呼",
                "遠隔へ切り替えた後に自動点呼として記録されてはいけない"
            );
        }
    );

    test_case!("理由が空なら 400", {
        let session =
            start_pre_operation_session(&client, &base_url, &auth, employee_id, state.pool()).await;
        let session_id = session["id"].as_str().unwrap();

        let res = client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/escalate-remote"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "reason": "  " }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });

    test_case!(
        "通常点呼のセッションは遠隔へ切り替えられない (別経路の振る舞いを変えない)",
        {
            let session =
                start_pre_operation_session(&client, &base_url, &auth, employee_id, state.pool())
                    .await;
            let session_id = session["id"].as_str().unwrap();
            let session_uuid = Uuid::parse_str(session_id).unwrap();

            sqlx::query(
                "UPDATE alc_api.tenko_sessions SET tenko_method = '通常点呼' WHERE id = $1",
            )
            .bind(session_uuid)
            .execute(state.pool())
            .await
            .unwrap();

            let res = client
                .put(format!(
                    "{base_url}/api/tenko/sessions/{session_id}/escalate-remote"
                ))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "reason": "測定不能" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 400);
        }
    );

    test_case!(
        "既に遠隔点呼へ切り替え済みのセッションは再度切り替えられない (二重にならない)",
        {
            let session =
                start_pre_operation_session(&client, &base_url, &auth, employee_id, state.pool())
                    .await;
            let session_id = session["id"].as_str().unwrap();

            let res = client
                .put(format!(
                    "{base_url}/api/tenko/sessions/{session_id}/escalate-remote"
                ))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "reason": "血圧計が繋がっていない" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200, "1 回目は通るはず");

            let res = client
                .put(format!(
                    "{base_url}/api/tenko/sessions/{session_id}/escalate-remote"
                ))
                .header("Authorization", &auth)
                .json(&serde_json::json!({ "reason": "その他" }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 400, "2 回目は弾くはず");
        }
    );

    test_case!("理由が上限 (200 文字) を超えたら 400", {
        let session =
            start_pre_operation_session(&client, &base_url, &auth, employee_id, state.pool()).await;
        let session_id = session["id"].as_str().unwrap();
        let too_long = "あ".repeat(201);

        let res = client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/escalate-remote"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "reason": too_long }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400);
    });

    test_case!("completed のセッションは切り替えられない", {
        let session =
            start_pre_operation_session(&client, &base_url, &auth, employee_id, state.pool()).await;
        let session_id = session["id"].as_str().unwrap();
        let session_uuid = Uuid::parse_str(session_id).unwrap();

        sqlx::query("UPDATE alc_api.tenko_sessions SET status = 'completed' WHERE id = $1")
            .bind(session_uuid)
            .execute(state.pool())
            .await
            .unwrap();

        let res = client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/escalate-remote"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "reason": "測定不能" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400, "終了済み (completed) は弾くはず");
    });

    test_case!("cancelled のセッションは切り替えられない", {
        let session =
            start_pre_operation_session(&client, &base_url, &auth, employee_id, state.pool()).await;
        let session_id = session["id"].as_str().unwrap();
        let session_uuid = Uuid::parse_str(session_id).unwrap();

        sqlx::query("UPDATE alc_api.tenko_sessions SET status = 'cancelled' WHERE id = $1")
            .bind(session_uuid)
            .execute(state.pool())
            .await
            .unwrap();

        let res = client
            .put(format!(
                "{base_url}/api/tenko/sessions/{session_id}/escalate-remote"
            ))
            .header("Authorization", &auth)
            .json(&serde_json::json!({ "reason": "測定不能" }))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 400, "終了済み (cancelled) は弾くはず");
    });
}

/// スケジュール無しで開始したセッションが tenko_method='遠隔点呼' で記録されることを固定する
/// (Refs ippoan/rust-alc-api#655)。合わせて、副次的な症状 — 血圧計を使わない端末での
/// 体温だけの提出が 400 で弾かれていた件 — の再発防止も確かめる。
#[tokio::test]
async fn no_schedule_start_session_sets_remote_method() {
    test_group!("スケジュール無し開始は tenko_method='遠隔点呼' で記録される (Refs #655)");

    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let client = reqwest::Client::new();

    let tenant = common::create_test_tenant(state.pool(), "Remote Tenko Method Test").await;
    let jwt = common::create_test_jwt(tenant, "admin");
    let auth = format!("Bearer {jwt}");

    let employee =
        common::create_test_employee(&client, &base_url, &auth, "Remote Driver", "REM001").await;
    let employee_id = employee["id"].as_str().unwrap();

    test_case!(
        "スケジュール無しで開始すると tenko_method='遠隔点呼' で作られる (DB にも残る)",
        {
            let res = client
                .post(format!("{base_url}/api/tenko/sessions/start"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "employee_id": employee_id,
                    "tenko_type": "pre_operation",
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["status"], "medical_pending");
            assert_eq!(
                body["tenko_method"], "遠隔点呼",
                "遠隔点呼として開始したセッションが自動点呼として記録されてはいけない (Refs #655)"
            );

            let session_id = Uuid::parse_str(body["id"].as_str().unwrap()).unwrap();
            let db_method: String =
                sqlx::query_scalar("SELECT tenko_method FROM alc_api.tenko_sessions WHERE id = $1")
                    .bind(session_id)
                    .fetch_one(state.pool())
                    .await
                    .unwrap();
            assert_eq!(db_method, "遠隔点呼");
        }
    );

    test_case!(
        "★ 症状の固定: 遠隔点呼のセッションは血圧を含まない体温だけの提出が 400 にならず成功する",
        {
            let res = client
                .post(format!("{base_url}/api/tenko/sessions/start"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "employee_id": employee_id,
                    "tenko_type": "pre_operation",
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let body: Value = res.json().await.unwrap();
            let session_id = body["id"].as_str().unwrap();

            let res = put_medical(&client, &base_url, &auth, session_id, false).await;
            assert_eq!(
                res.status(),
                200,
                "遠隔点呼 (血圧計を使わない端末) は体温だけで通るはず (本番で詰んでいた経路, Refs #655)"
            );
        }
    );
}

/// スケジュールありで開始したセッションは従来どおり tenko_method='自動点呼' のままで、
/// 血圧の無い体温提出は従来どおり 400 で弾かれることの回帰確認 (Refs ippoan/rust-alc-api#655)。
#[tokio::test]
async fn scheduled_start_session_keeps_auto_method() {
    test_group!("スケジュールあり開始は従来どおり tenko_method='自動点呼' (回帰, Refs #655)");

    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let client = reqwest::Client::new();

    let tenant = common::create_test_tenant(state.pool(), "Scheduled Tenko Method Test").await;
    let jwt = common::create_test_jwt(tenant, "admin");
    let auth = format!("Bearer {jwt}");

    let employee =
        common::create_test_employee(&client, &base_url, &auth, "Scheduled Driver", "SCH001").await;
    let employee_id = Uuid::parse_str(employee["id"].as_str().unwrap()).unwrap();

    let schedule_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alc_api.tenko_schedules
            (tenant_id, employee_id, tenko_type, responsible_manager_name, scheduled_at, instruction)
         VALUES ($1, $2, 'pre_operation', $3, NOW(), $4)
         RETURNING id",
    )
    .bind(tenant)
    .bind(employee_id)
    .bind("Manager")
    .bind("本日の注意事項")
    .fetch_one(state.pool())
    .await
    .unwrap();

    test_case!(
        "スケジュールありで開始すると tenko_method='自動点呼' のまま (退行がないこと)",
        {
            let res = client
                .post(format!("{base_url}/api/tenko/sessions/start"))
                .header("Authorization", &auth)
                .json(&serde_json::json!({
                    "employee_id": employee_id,
                    "schedule_id": schedule_id,
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 201);
            let body: Value = res.json().await.unwrap();
            assert_eq!(
                body["tenko_method"], "自動点呼",
                "スケジュールありの開始は従来どおり自動点呼のはず (退行, Refs #655)"
            );

            let session_id = body["id"].as_str().unwrap();
            let res = put_medical(&client, &base_url, &auth, session_id, false).await;
            assert_eq!(
                res.status(),
                400,
                "自動点呼は従来どおり血圧なしを弾くはず (退行がないこと)"
            );
        }
    );
}

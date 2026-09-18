//! 端末ごとに血圧計 (Omron) を使うかの設定 (`devices.bp_enabled`, migration 143) を
//! サーバ正本にする (Refs ippoan/alc-app-s3#135)。
//!
//! **実 DB でしか検証できない** — GET /api/devices/settings/{id} は
//! `alc_api.get_device_settings_by_id()` (SECURITY DEFINER 関数、063/114/143) 経由でしか
//! devices テーブルを読めない (`device_select_by_id` ポリシーは 063 で撤去済み)。
//! 列を DB に足しただけでは、この関数の列挙に乗せない限り応答に出てこない —
//! mock テスト (repository を丸ごと差し替える) はこの関数を経由しないので、
//! 関数側の列挙漏れを 1 行も検出できない。
//!
//! 回し方は他の DB integration テストと同じ:
//!   `make db-up && source .test-config && cargo test --test device_bp_enabled_test`
//! CI は ci.yml の `bazel-test-db` shard (postgres service + TEST_DATABASE_URL)。

#[macro_use]
mod common;

use common::create_device_via_url_flow;
use serde_json::Value;
use uuid::Uuid;

#[tokio::test]
async fn bp_enabled_defaults_false_and_round_trips_through_settings_endpoint() {
    test_group!("端末設定 (血圧計)");
    test_case!(
        "既定は false / PUT で true に更新できる / GET の応答に反映される (view/関数の列挙を含む)",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;

            let tenant = common::create_test_tenant(state.pool(), "BP Enabled Test").await;
            let jwt = common::create_test_jwt(tenant, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let (device_id, _code) = create_device_via_url_flow(&client, &base_url, &auth).await;

            // 1. 既定は false (migration 143 の DEFAULT false)
            let res = client
                .get(format!("{base_url}/api/devices/settings/{device_id}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert_eq!(
                body["bp_enabled"], false,
                "既定は false のはず (応答全体: {body})"
            );

            // 2. PUT で true に更新
            let res = client
                .put(format!("{base_url}/api/devices/{device_id}/call-settings"))
                .header("Authorization", auth.clone())
                .json(&serde_json::json!({
                    "call_enabled": false,
                    "bp_enabled": true
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 204);

            // 3. GET に反映される (get_device_settings_by_id の列挙に bp_enabled が
            //    乗っていないと、ここが false のまま = 列を足しただけでは検知できない罠)
            let res = client
                .get(format!("{base_url}/api/devices/settings/{device_id}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert_eq!(
                body["bp_enabled"], true,
                "PUT で true にした後は true が返るはず (応答全体: {body})"
            );

            // 4. call_enabled / call_schedule など既存の設定項目は今回のスコープ外
            //    (触らない) — PUT で call_enabled=false を送っても他の値が壊れて
            //    いないことだけ確認する
            assert_eq!(body["call_enabled"], false);
        }
    );
}

#[tokio::test]
async fn bp_enabled_unset_in_put_body_leaves_existing_value_unchanged() {
    test_group!("端末設定 (血圧計)");
    test_case!(
        "PUT で bp_enabled を省略した場合、UPDATE の COALESCE で既存値が保たれる",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;

            let tenant = common::create_test_tenant(state.pool(), "BP Enabled Coalesce").await;
            let jwt = common::create_test_jwt(tenant, "admin");
            let auth = format!("Bearer {jwt}");
            let client = reqwest::Client::new();

            let (device_id, _code) = create_device_via_url_flow(&client, &base_url, &auth).await;

            // まず true にする
            let res = client
                .put(format!("{base_url}/api/devices/{device_id}/call-settings"))
                .header("Authorization", auth.clone())
                .json(&serde_json::json!({ "call_enabled": false, "bp_enabled": true }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 204);

            // bp_enabled を省略した PUT (always_on だけ更新するような呼び出しを模す)
            let res = client
                .put(format!("{base_url}/api/devices/{device_id}/call-settings"))
                .header("Authorization", auth)
                .json(&serde_json::json!({ "call_enabled": true }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 204);

            let res = client
                .get(format!("{base_url}/api/devices/settings/{device_id}"))
                .send()
                .await
                .unwrap();
            let body: Value = res.json().await.unwrap();
            assert_eq!(
                body["bp_enabled"], true,
                "bp_enabled を省略した PUT で既存の true が消えてはいけない (応答全体: {body})"
            );
        }
    );
}

/// 自動点呼 (キオスク) の業務前セッションを開始する
/// (`tests/tenko_bp_required_escalate_test.rs` の同名ヘルパーと同じ作り。
/// 各 integration test ファイルは独立 binary のため共有できず、ここに複製する)。
async fn start_pre_operation_session(
    client: &reqwest::Client,
    base_url: &str,
    auth: &str,
    employee_id: &str,
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
    res.json().await.unwrap()
}

async fn put_medical(
    client: &reqwest::Client,
    base_url: &str,
    auth: &str,
    session_id: &str,
    with_bp: bool,
    device_id: Option<&str>,
) -> reqwest::Response {
    let mut body = serde_json::json!({ "temperature": 36.5 });
    if with_bp {
        body["systolic"] = Value::from(120);
        body["diastolic"] = Value::from(80);
    }
    if let Some(device_id) = device_id {
        body["device_id"] = Value::from(device_id);
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

/// submit_medical の血圧必須判定は devices.bp_enabled (migration 143) を正本にする
/// (Refs ippoan/alc-app#322)。`bp_enabled` の真偽値自体はクライアントに申告させず、
/// device_id だけを受け取ってサーバが DB を引く — この境界を固定する。
#[tokio::test]
async fn auto_tenko_bp_required_follows_device_bp_enabled() {
    test_group!("自動点呼の血圧必須は devices.bp_enabled が正本 (Refs ippoan/alc-app#322)");

    let state = common::setup_app_state().await;
    let base_url = common::spawn_test_server(state.clone()).await;
    let client = reqwest::Client::new();

    let tenant = common::create_test_tenant(state.pool(), "Device BP Required Test").await;
    let jwt = common::create_test_jwt(tenant, "admin");
    let auth = format!("Bearer {jwt}");

    let employee =
        common::create_test_employee(&client, &base_url, &auth, "BP322 Driver", "BP322-01").await;
    let employee_id = employee["id"].as_str().unwrap();

    let (device_id, _code) = create_device_via_url_flow(&client, &base_url, &auth).await;

    // セッションは tenko_method の既定 ('遠隔点呼', Refs #655) では血圧を問わないため、
    // このファイルの分岐を確かめるには '自動点呼' へ明示的に上書きする
    // (`tenko_bp_required_escalate_test.rs` と同じ作法)。
    async fn as_auto_tenko(pool: &sqlx::PgPool, session: &Value) {
        let session_id = Uuid::parse_str(session["id"].as_str().unwrap()).unwrap();
        sqlx::query("UPDATE alc_api.tenko_sessions SET tenko_method = '自動点呼' WHERE id = $1")
            .bind(session_id)
            .execute(pool)
            .await
            .unwrap();
    }

    test_case!(
        "bp_enabled=false (既定) の端末は血圧なしで通る — #322 の症状固定",
        {
            let session = start_pre_operation_session(&client, &base_url, &auth, employee_id).await;
            as_auto_tenko(state.pool(), &session).await;
            let session_id = session["id"].as_str().unwrap();

            let res = put_medical(
                &client,
                &base_url,
                &auth,
                session_id,
                false,
                Some(&device_id),
            )
            .await;
            assert_eq!(
                res.status(),
                200,
                "血圧計を使わない端末 (bp_enabled=false) は血圧なしで通るはず"
            );
        }
    );

    test_case!(
        "bp_enabled=true の端末は従来どおり血圧なしを 400 で弾く (回帰) / 血圧ありは 200",
        {
            let res = client
                .put(format!("{base_url}/api/devices/{device_id}/call-settings"))
                .header("Authorization", auth.clone())
                .json(&serde_json::json!({ "call_enabled": false, "bp_enabled": true }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 204);

            let session = start_pre_operation_session(&client, &base_url, &auth, employee_id).await;
            as_auto_tenko(state.pool(), &session).await;
            let session_id = session["id"].as_str().unwrap();

            let res = put_medical(
                &client,
                &base_url,
                &auth,
                session_id,
                false,
                Some(&device_id),
            )
            .await;
            assert_eq!(
                res.status(),
                400,
                "bp_enabled=true の端末は従来どおり血圧なしを弾くはず (退行がないこと)"
            );

            let res = put_medical(
                &client,
                &base_url,
                &auth,
                session_id,
                true,
                Some(&device_id),
            )
            .await;
            assert_eq!(res.status(), 200, "血圧ありなら通るはず");
        }
    );

    test_case!(
        "自動点呼でない場合は device_id / bp_enabled に関わらず従来どおり血圧なしで通る",
        {
            // 直前のケースで devices.bp_enabled は true のまま
            let session = start_pre_operation_session(&client, &base_url, &auth, employee_id).await;
            let session_id = session["id"].as_str().unwrap();
            let session_uuid = Uuid::parse_str(session_id).unwrap();

            sqlx::query(
                "UPDATE alc_api.tenko_sessions SET tenko_method = '通常点呼' WHERE id = $1",
            )
            .bind(session_uuid)
            .execute(state.pool())
            .await
            .unwrap();

            let res = put_medical(
                &client,
                &base_url,
                &auth,
                session_id,
                false,
                Some(&device_id),
            )
            .await;
            assert_eq!(
                res.status(),
                200,
                "通常点呼は device_id があっても血圧なしで通るはず (退行がないこと)"
            );
        }
    );
}

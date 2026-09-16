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

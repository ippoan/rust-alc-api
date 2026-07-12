//! POST /api/hub/measurements (CoreS3 ハブ ingest、Refs #564) の DB integration テスト。
//!
//! 経路: cf-alc-recorder →(auth-worker /alc-internal-proxy)→ 本 API。
//! `internal_shared_secret_router` 配下 (X-Internal-Shared-Secret + X-Tenant-ID) の
//! 実 DB での冪等性 (UNIQUE (tenant_id, device_id, seq)) とテナント分離を固定する。

#[macro_use]
mod common;

use serde_json::{json, Value};
use uuid::Uuid;

fn measurement(device_id: &str, seq: i64, kind: &str) -> Value {
    json!({
        "device_id": device_id,
        "kind": kind,
        "seq": seq,
        "recorded_at_ms": 1_752_300_000_000i64,
        "payload": { "type": "temperature", "value": 36.5, "unit": "celsius" }
    })
}

async fn post_measurements(
    client: &reqwest::Client,
    base_url: &str,
    tenant_id: Uuid,
    body: &Value,
) -> reqwest::Response {
    client
        .post(format!("{base_url}/api/hub/measurements"))
        .header(
            "X-Internal-Shared-Secret",
            common::TEST_INTERNAL_SHARED_SECRET,
        )
        .header("X-Tenant-ID", tenant_id.to_string())
        .json(body)
        .send()
        .await
        .unwrap()
}

async fn count_rows(pool: &sqlx::PgPool, tenant_id: Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM hub_measurements WHERE tenant_id = $1")
        .bind(tenant_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn test_hub_measurements_ingest_idempotency_and_batch() {
    test_group!("hub measurements ingest");
    test_case!("seq 重複冪等 + バッチ", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant = common::create_test_tenant(state.pool(), "Hub Ingest A").await;
        let client = reqwest::Client::new();

        // バッチ 3 件 → 全部 insert
        let res = post_measurements(
            &client,
            &base_url,
            tenant,
            &json!([
                measurement("hub-dev-1", 1, "temperature"),
                measurement("hub-dev-1", 2, "blood_pressure"),
                measurement("hub-dev-1", 3, "alcohol"),
            ]),
        )
        .await;
        assert_eq!(res.status(), 201);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["inserted"], 3);
        assert_eq!(body["duplicates"], 0);
        assert_eq!(count_rows(state.pool(), tenant).await, 3);

        // 同じ seq の再送 (ACK 未達再送) → ON CONFLICT DO NOTHING で冪等
        let res = post_measurements(
            &client,
            &base_url,
            tenant,
            &json!([measurement("hub-dev-1", 2, "blood_pressure")]),
        )
        .await;
        assert_eq!(res.status(), 201);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["inserted"], 0);
        assert_eq!(body["duplicates"], 1);
        assert_eq!(count_rows(state.pool(), tenant).await, 3);

        // 新規 + 重複の混在バッチ → 新規だけ insert
        let res = post_measurements(
            &client,
            &base_url,
            tenant,
            &json!([
                measurement("hub-dev-1", 3, "alcohol"),
                measurement("hub-dev-1", 4, "fc1200_raw"),
            ]),
        )
        .await;
        assert_eq!(res.status(), 201);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["inserted"], 1);
        assert_eq!(body["duplicates"], 1);
        assert_eq!(count_rows(state.pool(), tenant).await, 4);

        // 単発 object も受ける + 別 device なら同じ seq でも insert される
        let res = post_measurements(
            &client,
            &base_url,
            tenant,
            &measurement("hub-dev-2", 1, "alcohol"),
        )
        .await;
        assert_eq!(res.status(), 201);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["inserted"], 1);

        // recorded_at は recorded_at_ms 由来で保存される
        let recorded: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
            "SELECT recorded_at FROM hub_measurements
              WHERE tenant_id = $1 AND device_id = 'hub-dev-1' AND seq = 1",
        )
        .bind(tenant)
        .fetch_one(state.pool())
        .await
        .unwrap();
        assert_eq!(
            recorded,
            chrono::DateTime::from_timestamp_millis(1_752_300_000_000)
        );
    });
}

#[tokio::test]
async fn test_hub_measurements_tenant_isolation() {
    test_group!("hub measurements ingest");
    test_case!(
        "テナント分離 (UNIQUE は tenant 単位 / 行はヘッダー tenant に紐付く)",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_a = common::create_test_tenant(state.pool(), "Hub Iso A").await;
            let tenant_b = common::create_test_tenant(state.pool(), "Hub Iso B").await;
            let client = reqwest::Client::new();

            // 同じ (device_id, seq) でも tenant が違えば衝突しない
            let res = post_measurements(
                &client,
                &base_url,
                tenant_a,
                &json!([measurement("hub-dev-1", 100, "alcohol")]),
            )
            .await;
            assert_eq!(res.status(), 201);
            let res = post_measurements(
                &client,
                &base_url,
                tenant_b,
                &json!([measurement("hub-dev-1", 100, "alcohol")]),
            )
            .await;
            assert_eq!(res.status(), 201);
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["inserted"], 1);

            // 行は X-Tenant-ID の tenant にだけ紐付く (ペイロードは tenant を運ばない)
            assert_eq!(count_rows(state.pool(), tenant_a).await, 1);
            assert_eq!(count_rows(state.pool(), tenant_b).await, 1);
        }
    );
}

#[tokio::test]
async fn test_hub_measurements_auth_and_validation() {
    test_group!("hub measurements ingest");
    test_case!(
        "shared secret / tenant 欠落は 401、allowlist 外 kind は 400",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant = common::create_test_tenant(state.pool(), "Hub Auth A").await;
            let client = reqwest::Client::new();

            // secret なし → 401
            let res = client
                .post(format!("{base_url}/api/hub/measurements"))
                .header("X-Tenant-ID", tenant.to_string())
                .json(&json!([measurement("hub-dev-1", 1, "alcohol")]))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 401);

            // X-Tenant-ID なし → 401
            let res = client
                .post(format!("{base_url}/api/hub/measurements"))
                .header(
                    "X-Internal-Shared-Secret",
                    common::TEST_INTERNAL_SHARED_SECRET,
                )
                .json(&json!([measurement("hub-dev-1", 1, "alcohol")]))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 401);

            // allowlist 外 kind → 400 (DB には入らない)
            let res = post_measurements(
                &client,
                &base_url,
                tenant,
                &json!([measurement("hub-dev-1", 1, "not-a-kind")]),
            )
            .await;
            assert_eq!(res.status(), 400);
            assert_eq!(count_rows(state.pool(), tenant).await, 0);
        }
    );
}

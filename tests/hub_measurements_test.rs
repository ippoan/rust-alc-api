//! `/api/hub/measurements` (CoreS3 ハブ、Refs #564 ingest / #592 read) の DB integration テスト。
//!
//! - POST … cf-alc-recorder →(auth-worker /alc-internal-proxy)→ 本 API。
//!   `internal_shared_secret_router` 配下 (X-Internal-Shared-Secret + X-Tenant-ID) の
//!   実 DB での冪等性 (UNIQUE (tenant_id, device_id, seq)) とテナント分離を固定する。
//! - GET … テナント認証付き router (X-Tenant-ID)。絞り込み・created_at DESC の
//!   ページング・**テナント分離**を実 DB で固定する。

#[macro_use]
mod common;

use serde_json::{json, Value};
use uuid::Uuid;

/// session_id 付き (1 回の点呼で束ねられる形、Refs ippoan/alc-app-s3#112)。
fn measurement_in_session(device_id: &str, seq: i64, kind: &str, session_id: &str) -> Value {
    let mut m = measurement(device_id, seq, kind);
    m["session_id"] = json!(session_id);
    m
}

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

/// GET /api/hub/measurements (テナント認証付き router = X-Tenant-ID のみ)。
async fn get_measurements(
    client: &reqwest::Client,
    base_url: &str,
    tenant_id: Uuid,
    query: &str,
) -> reqwest::Response {
    client
        .get(format!("{base_url}/api/hub/measurements?{query}"))
        .header("X-Tenant-ID", tenant_id.to_string())
        .send()
        .await
        .unwrap()
}

/// created_at は now() 既定なので、期間絞り込み・並び順を決定的に検証するために
/// 実 DB 側で明示的に上書きする。
async fn set_created_at(pool: &sqlx::PgPool, tenant_id: Uuid, seq: i64, iso: &str) {
    let ts: chrono::DateTime<chrono::Utc> = iso.parse().unwrap();
    sqlx::query("UPDATE hub_measurements SET created_at = $1 WHERE tenant_id = $2 AND seq = $3")
        .bind(ts)
        .bind(tenant_id)
        .bind(seq)
        .execute(pool)
        .await
        .unwrap();
}

fn seqs(body: &Value) -> Vec<i64> {
    body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["seq"].as_i64().unwrap())
        .collect()
}

#[tokio::test]
async fn test_hub_measurements_list_filters_and_paging() {
    test_group!("hub measurements read");
    test_case!(
        "device_id / kind / 期間の絞り込みと created_at DESC ページング",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant = common::create_test_tenant(state.pool(), "Hub Read A").await;
            let client = reqwest::Client::new();

            let res = post_measurements(
                &client,
                &base_url,
                tenant,
                &json!([
                    measurement("hub-dev-1", 1, "temperature"),
                    measurement("hub-dev-1", 2, "alcohol"),
                    measurement("hub-dev-1", 3, "alcohol"),
                    measurement("hub-dev-2", 4, "temperature"),
                ]),
            )
            .await;
            assert_eq!(res.status(), 201);

            // seq 1..4 を 1 日ずつずらす (created_at DESC = seq 降順になる)
            for (seq, iso) in [
                (1, "2026-08-01T00:00:00Z"),
                (2, "2026-08-02T00:00:00Z"),
                (3, "2026-08-03T00:00:00Z"),
                (4, "2026-08-04T00:00:00Z"),
            ] {
                set_created_at(state.pool(), tenant, seq, iso).await;
            }

            // 絞り込みなし → 全件が created_at DESC
            let res = get_measurements(&client, &base_url, tenant, "").await;
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert_eq!(seqs(&body), vec![4, 3, 2, 1]);
            assert_eq!(body["limit"], 50);
            assert_eq!(body["offset"], 0);
            assert_eq!(body["has_more"], false);
            // payload は JSONB 素通し、recorded_at は ingest 時の recorded_at_ms 由来
            assert_eq!(body["items"][0]["payload"]["value"], 36.5);
            assert_eq!(body["items"][0]["device_id"], "hub-dev-2");
            assert!(body["items"][0]["recorded_at"].is_string());

            // device_id 絞り込み
            let res = get_measurements(&client, &base_url, tenant, "device_id=hub-dev-1").await;
            let body: Value = res.json().await.unwrap();
            assert_eq!(seqs(&body), vec![3, 2, 1]);

            // kind 絞り込み
            let res = get_measurements(&client, &base_url, tenant, "kind=alcohol").await;
            let body: Value = res.json().await.unwrap();
            assert_eq!(seqs(&body), vec![3, 2]);

            // 期間 (created_at の閉区間)
            let res = get_measurements(
                &client,
                &base_url,
                tenant,
                "from=2026-08-02T00:00:00Z&to=2026-08-03T00:00:00Z",
            )
            .await;
            let body: Value = res.json().await.unwrap();
            assert_eq!(seqs(&body), vec![3, 2]);

            // 絞り込みの組み合わせ
            let res = get_measurements(
                &client,
                &base_url,
                tenant,
                "device_id=hub-dev-1&kind=alcohol&from=2026-08-03T00:00:00Z",
            )
            .await;
            let body: Value = res.json().await.unwrap();
            assert_eq!(seqs(&body), vec![3]);

            // ページング (limit + offset)。has_more は次ページの有無
            let res = get_measurements(&client, &base_url, tenant, "limit=2").await;
            let body: Value = res.json().await.unwrap();
            assert_eq!(seqs(&body), vec![4, 3]);
            assert_eq!(body["has_more"], true);

            let res = get_measurements(&client, &base_url, tenant, "limit=2&offset=2").await;
            let body: Value = res.json().await.unwrap();
            assert_eq!(seqs(&body), vec![2, 1]);
            assert_eq!(body["has_more"], false);
            assert_eq!(body["offset"], 2);

            // limit は上限で clamp される (実効値がレスポンスに出る)
            let res = get_measurements(&client, &base_url, tenant, "limit=99999").await;
            let body: Value = res.json().await.unwrap();
            assert_eq!(body["limit"], 200);
            assert_eq!(seqs(&body), vec![4, 3, 2, 1]);

            // allowlist 外 kind / 逆転した期間は 400、X-Tenant-ID なしは 401
            let res = get_measurements(&client, &base_url, tenant, "kind=not-a-kind").await;
            assert_eq!(res.status(), 400);
            let res = get_measurements(
                &client,
                &base_url,
                tenant,
                "from=2026-08-04T00:00:00Z&to=2026-08-01T00:00:00Z",
            )
            .await;
            assert_eq!(res.status(), 400);
            let res = client
                .get(format!("{base_url}/api/hub/measurements"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 401);
        }
    );
}

#[tokio::test]
async fn test_hub_measurements_list_tenant_isolation() {
    test_group!("hub measurements read");
    test_case!(
        "別テナントの行は一覧に出ない (RLS 任せにせずテストで固定)",
        {
            let state = common::setup_app_state().await;
            let base_url = common::spawn_test_server(state.clone()).await;
            let tenant_a = common::create_test_tenant(state.pool(), "Hub Read Iso A").await;
            let tenant_b = common::create_test_tenant(state.pool(), "Hub Read Iso B").await;
            let client = reqwest::Client::new();

            // 同じ device_id / seq を両テナントに入れる (混ざれば必ず検知できる形)
            for tenant in [tenant_a, tenant_b] {
                let res = post_measurements(
                    &client,
                    &base_url,
                    tenant,
                    &json!([measurement("hub-dev-shared", 1, "alcohol")]),
                )
                .await;
                assert_eq!(res.status(), 201);
            }
            // B にだけもう 1 件足しておく (件数でも区別できるようにする)
            let res = post_measurements(
                &client,
                &base_url,
                tenant_b,
                &json!([measurement("hub-dev-shared", 2, "temperature")]),
            )
            .await;
            assert_eq!(res.status(), 201);

            // A から見えるのは A の 1 件だけ
            let res = get_measurements(&client, &base_url, tenant_a, "").await;
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            assert_eq!(seqs(&body), vec![1]);
            assert_eq!(body["items"][0]["tenant_id"], tenant_a.to_string());

            // B から見えるのは B の 2 件だけ
            let res = get_measurements(&client, &base_url, tenant_b, "").await;
            let body: Value = res.json().await.unwrap();
            assert_eq!(seqs(&body), vec![2, 1]);
            assert!(body["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|i| i["tenant_id"] == tenant_b.to_string()));

            // 絞り込みを付けても他テナントの行は漏れない
            let res = get_measurements(
                &client,
                &base_url,
                tenant_a,
                "device_id=hub-dev-shared&kind=temperature",
            )
            .await;
            let body: Value = res.json().await.unwrap();
            assert_eq!(seqs(&body), Vec::<i64>::new());
        }
    );
}

#[tokio::test]
async fn test_hub_measurements_session_id_roundtrip_and_filter() {
    test_group!("hub measurements read");
    test_case!("session_id で 1 回の点呼を束ねて引ける", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant = common::create_test_tenant(state.pool(), "Hub Session A").await;
        let client = reqwest::Client::new();

        // 点呼 1 (s10) の 3 点 + 点呼 2 (s20) の 1 点 + 点呼外の単発 (session_id 無し)
        let res = post_measurements(
            &client,
            &base_url,
            tenant,
            &json!([
                measurement_in_session("hub-dev-1", 10, "alcohol", "s10"),
                measurement_in_session("hub-dev-1", 11, "temperature", "s10"),
                measurement_in_session("hub-dev-1", 12, "blood_pressure", "s10"),
                measurement_in_session("hub-dev-1", 20, "alcohol", "s20"),
                measurement("hub-dev-1", 30, "temperature"),
            ]),
        )
        .await;
        assert_eq!(res.status(), 201);
        assert_eq!(res.json::<Value>().await.unwrap()["inserted"], 5);

        // session_id は保存され、レスポンスに出る
        let res = get_measurements(&client, &base_url, tenant, "session_id=s10").await;
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(seqs(&body), vec![12, 11, 10]);
        assert!(body["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["session_id"] == "s10"));

        // 別セッションは混ざらない
        let res = get_measurements(&client, &base_url, tenant, "session_id=s20").await;
        let body: Value = res.json().await.unwrap();
        assert_eq!(seqs(&body), vec![20]);

        // 点呼外の単発は session_id = null (欠損ではなく「セッション不明」)
        let res = get_measurements(&client, &base_url, tenant, "kind=temperature").await;
        let body: Value = res.json().await.unwrap();
        let single = body["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["seq"] == 30)
            .expect("seq=30");
        assert!(single["session_id"].is_null());

        // 他の絞り込みと併用できる
        let res = get_measurements(&client, &base_url, tenant, "session_id=s10&kind=alcohol").await;
        let body: Value = res.json().await.unwrap();
        assert_eq!(seqs(&body), vec![10]);

        // 不正な session_id (記号混じり) は 400
        let res = get_measurements(&client, &base_url, tenant, "session_id=s%2F10").await;
        assert_eq!(res.status(), 400);

        // ingest 側も同じ検証 — 不正な session_id は 400 で DB に入らない
        let mut bad = measurement("hub-dev-9", 1, "alcohol");
        bad["session_id"] = json!("bad id");
        let res = post_measurements(&client, &base_url, tenant, &json!([bad])).await;
        assert_eq!(res.status(), 400);
        assert_eq!(count_rows(state.pool(), tenant).await, 5);
    });
}

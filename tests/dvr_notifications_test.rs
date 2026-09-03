//! `/api/dvr/notifications` と `/api/dvr/files/{id}` (theearth DVR 動画通知の受け皿、
//! Refs ohishi-exp/nuxt-dtako-admin#1094) の DB integration テスト。
//!
//! 両方とも `internal_shared_secret_router` 配下 (X-Internal-Shared-Secret +
//! X-Tenant-ID)。この class は **caller が tenant を名乗る**ので、
//!
//! - 自然キー `UNIQUE (tenant_id, serial_no, file_name)` の冪等性
//! - 応答の `pending` が「新規 + まだ pending かつ attempts が上限未満の既存行」になること
//! - **`{id}` を X-Tenant-ID と対で突合していること** (別テナントの id は 404)
//! - 32MB 超は 413 + `file_status='failed'`
//!
//! を実 DB で固定する。

#[macro_use]
mod common;

use serde_json::{json, Value};
use uuid::Uuid;

/// ハンドラ側の `MAX_FILE_BYTES` と同じ値 (32MB = Cloud Run の HTTP body 上限)。
const MAX_FILE_BYTES: usize = 32 * 1024 * 1024;

/// ハンドラ側の `MAX_FILE_ATTEMPTS` と同じ値。
const MAX_FILE_ATTEMPTS: i32 = 6;

fn notification(serial_no: &str, file_name: &str) -> Value {
    json!({
        "serial_no": serial_no,
        "file_name": file_name,
        "vehicle_cd": "1234",
        "vehicle_name": "test-vehicle",
        "driver_name": "test-driver",
        "event_type": "sudden-brake",
        "dvr_datetime": "2026-09-03T01:23:45+09:00",
        "source_url": "https://example.test/dvr/a"
    })
}

async fn post_notifications(
    client: &reqwest::Client,
    base_url: &str,
    tenant_id: Uuid,
    items: Value,
) -> reqwest::Response {
    client
        .post(format!("{base_url}/api/dvr/notifications"))
        .header(
            "X-Internal-Shared-Secret",
            common::TEST_INTERNAL_SHARED_SECRET,
        )
        .header("X-Tenant-ID", tenant_id.to_string())
        .json(&json!({ "items": items }))
        .send()
        .await
        .unwrap()
}

async fn post_file(
    client: &reqwest::Client,
    base_url: &str,
    tenant_id: Uuid,
    id: &str,
    body: Vec<u8>,
) -> reqwest::Response {
    client
        .post(format!("{base_url}/api/dvr/files/{id}"))
        .header(
            "X-Internal-Shared-Secret",
            common::TEST_INTERNAL_SHARED_SECRET,
        )
        .header("X-Tenant-ID", tenant_id.to_string())
        .header("Content-Type", "application/octet-stream")
        .body(body)
        .send()
        .await
        .unwrap()
}

/// テスト DB から 1 行の (file_status, attempts, r2_key, size_bytes) を読む。
/// 素の pool (superuser) で引くので RLS を経由しない = ハンドラ側の tenant 条件を
/// 検査するための「外から見た真値」になる。
async fn row_state(
    pool: &sqlx::PgPool,
    id: Uuid,
) -> (String, i32, Option<String>, Option<i64>, Option<String>) {
    sqlx::query_as(
        "SELECT file_status, attempts, r2_key, size_bytes, last_error
           FROM dvr_notifications WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn pending_names(body: &Value) -> Vec<String> {
    body["pending"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["file_name"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn test_dvr_notifications_ingest_idempotency_and_pending() {
    test_group!("dvr notifications ingest");
    test_case!("自然キー冪等 + pending の再掲", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant = common::create_test_tenant(state.pool(), "DVR Ingest A").await;
        let client = reqwest::Client::new();

        // 新規 2 件 → 両方 insert され、両方 pending に載る
        let res = post_notifications(
            &client,
            &base_url,
            tenant,
            json!([
                notification("SN-0001", "a.vdf"),
                notification("SN-0001", "b.vdf"),
            ]),
        )
        .await;
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["inserted"], 2);
        assert_eq!(body["skipped"], 0);
        let mut names = pending_names(&body);
        names.sort();
        assert_eq!(names, vec!["a.vdf", "b.vdf"]);

        // 同じ 2 件の再送 → insert 0 / skipped 2。まだ pending なので 2 件とも再掲される
        // (廃止元の RetryPendingDownloads 相当をこの応答が兼ねる)
        let res = post_notifications(
            &client,
            &base_url,
            tenant,
            json!([
                notification("SN-0001", "a.vdf"),
                notification("SN-0001", "b.vdf"),
            ]),
        )
        .await;
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["inserted"], 0);
        assert_eq!(body["skipped"], 2);
        assert_eq!(pending_names(&body).len(), 2);

        let a_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM dvr_notifications WHERE tenant_id = $1 AND file_name = 'a.vdf'",
        )
        .bind(tenant)
        .fetch_one(state.pool())
        .await
        .unwrap();

        // a.vdf の実体を保存 → stored になり、以後 pending には載らない
        let res = post_file(
            &client,
            &base_url,
            tenant,
            &a_id.to_string(),
            vec![1u8; 2048],
        )
        .await;
        assert_eq!(res.status(), 200);
        let stored: Value = res.json().await.unwrap();
        assert_eq!(stored["file_status"], "stored");
        assert_eq!(stored["size"], 2048);
        assert_eq!(stored["r2_key"], format!("{tenant}/dvr/SN-0001/a.vdf"));

        let (status, attempts, r2_key, size, last_error) = row_state(state.pool(), a_id).await;
        assert_eq!(status, "stored");
        assert_eq!(attempts, 0);
        assert_eq!(r2_key.unwrap(), format!("{tenant}/dvr/SN-0001/a.vdf"));
        assert_eq!(size.unwrap(), 2048);
        assert!(last_error.is_none());

        // R2 (テストでは MockStorage) に .vdf がそのまま置かれている (mp4 変換しない)
        let key = format!("{tenant}/dvr/SN-0001/a.vdf");
        let stored_bytes = state
            .dtako_storage
            .as_ref()
            .unwrap()
            .download(&key)
            .await
            .unwrap();
        assert_eq!(stored_bytes, vec![1u8; 2048]);

        let res = post_notifications(
            &client,
            &base_url,
            tenant,
            json!([
                notification("SN-0001", "a.vdf"),
                notification("SN-0001", "b.vdf"),
            ]),
        )
        .await;
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["skipped"], 2);
        assert_eq!(pending_names(&body), vec!["b.vdf"]);

        // attempts が上限に達した行も pending から落ちる (恒久失敗の足切り)
        sqlx::query(
            "UPDATE dvr_notifications SET attempts = $2
              WHERE tenant_id = $1 AND file_name = 'b.vdf'",
        )
        .bind(tenant)
        .bind(MAX_FILE_ATTEMPTS)
        .execute(state.pool())
        .await
        .unwrap();

        let res = post_notifications(
            &client,
            &base_url,
            tenant,
            json!([notification("SN-0001", "b.vdf")]),
        )
        .await;
        let body: Value = res.json().await.unwrap();
        assert_eq!(body["skipped"], 1);
        assert!(pending_names(&body).is_empty());
    });
}

#[tokio::test]
async fn test_dvr_notifications_ingest_rejects_bad_input() {
    test_group!("dvr notifications ingest 入力検証");
    test_case!("空バッチ / theearth 由来の不正なキーは 400", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant = common::create_test_tenant(state.pool(), "DVR Ingest B").await;
        let client = reqwest::Client::new();

        // 空バッチ (relay の bug を無言の 200 で隠さない)
        let res = post_notifications(&client, &base_url, tenant, json!([])).await;
        assert_eq!(res.status(), 400);

        // file_name に path separator / traversal
        for bad in ["../../etc/passwd", "a/b.vdf", ""] {
            let res = post_notifications(
                &client,
                &base_url,
                tenant,
                json!([notification("SN-1", bad)]),
            )
            .await;
            assert_eq!(res.status(), 400, "file_name={bad:?}");
        }

        // 1 行も入っていない
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM dvr_notifications WHERE tenant_id = $1")
                .bind(tenant)
                .fetch_one(state.pool())
                .await
                .unwrap();
        assert_eq!(count, 0);
    });
}

/// **テナント分離**: `POST /api/dvr/files/{id}` は `{id}` を `X-Tenant-ID` と対で
/// 突合しなければならない (`WHERE id = $1 AND tenant_id = $2`)。
///
/// この router は caller が tenant を名乗る class なので、id だけで行を引くと
/// 別テナントの行を上書きできる。「無い」と「他人のもの」は区別させず 404。
#[tokio::test]
async fn test_dvr_file_upload_rejects_cross_tenant_id() {
    test_group!("dvr file upload テナント分離");
    test_case!("別 tenant の id は 404 / 同 tenant なら 200", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant_a = common::create_test_tenant(state.pool(), "DVR Tenant A").await;
        let tenant_b = common::create_test_tenant(state.pool(), "DVR Tenant B").await;
        let client = reqwest::Client::new();

        // tenant A の行を起票
        let res = post_notifications(
            &client,
            &base_url,
            tenant_a,
            json!([notification("SN-A", "secret.vdf")]),
        )
        .await;
        assert_eq!(res.status(), 200);
        let body: Value = res.json().await.unwrap();
        let a_id = body["pending"][0]["id"].as_str().unwrap().to_string();

        // tenant B を名乗って A の id へ書き込む → 404 (行は一切変わらない)
        let res = post_file(&client, &base_url, tenant_b, &a_id, vec![9u8; 128]).await;
        assert_eq!(res.status(), 404);

        let a_uuid: Uuid = a_id.parse().unwrap();
        let (status, attempts, r2_key, size, _) = row_state(state.pool(), a_uuid).await;
        assert_eq!(status, "pending");
        assert_eq!(attempts, 0);
        assert!(r2_key.is_none());
        assert!(size.is_none());

        // 存在しない id も同じく 404 (「無い」と「他人のもの」を区別させない)
        let res = post_file(
            &client,
            &base_url,
            tenant_b,
            &Uuid::new_v4().to_string(),
            vec![9u8; 128],
        )
        .await;
        assert_eq!(res.status(), 404);

        // 本来の所有者 (tenant A) なら 200
        let res = post_file(&client, &base_url, tenant_a, &a_id, vec![9u8; 128]).await;
        assert_eq!(res.status(), 200);
        let (status, _, r2_key, size, _) = row_state(state.pool(), a_uuid).await;
        assert_eq!(status, "stored");
        assert_eq!(r2_key.unwrap(), format!("{tenant_a}/dvr/SN-A/secret.vdf"));
        assert_eq!(size.unwrap(), 128);
    });
}

#[tokio::test]
async fn test_dvr_file_upload_rejects_oversize() {
    test_group!("dvr file upload サイズ上限");
    test_case!("32MB 超は 413 + file_status='failed'", {
        let state = common::setup_app_state().await;
        let base_url = common::spawn_test_server(state.clone()).await;
        let tenant = common::create_test_tenant(state.pool(), "DVR Oversize").await;
        let client = reqwest::Client::new();

        let res = post_notifications(
            &client,
            &base_url,
            tenant,
            json!([notification("SN-BIG", "big.vdf")]),
        )
        .await;
        let body: Value = res.json().await.unwrap();
        let id: Uuid = body["pending"][0]["id"].as_str().unwrap().parse().unwrap();

        let res = post_file(
            &client,
            &base_url,
            tenant,
            &id.to_string(),
            vec![0u8; MAX_FILE_BYTES + 1],
        )
        .await;
        assert_eq!(res.status(), 413);

        let (status, attempts, r2_key, size, last_error) = row_state(state.pool(), id).await;
        assert_eq!(status, "failed");
        assert_eq!(attempts, 1);
        assert!(r2_key.is_none());
        assert!(size.is_none());
        assert!(last_error.unwrap().contains("exceeds"));
    });
}

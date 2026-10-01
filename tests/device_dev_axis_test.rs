//! dev端末 (開発用の鍵) の印 `X-Device-Dev: 1` が DB の接続まで通り、dev の行と本番の行が
//! 分かれることを実 DB で固定する。
//!
//! Refs ippoan/alc-app#387
//!
//! backend は INSERT / SELECT を書き換えていない。認証を通した要求の印を接続の設定
//! `app.device_dev` に立てるだけで、行の出し分けは列の既定値と RLS (alc-migrations 153) が
//! 担う。**実 DB でしか検証できない** — repository を差し替える mock テストでは 1 行も通らない。
//!
//! ほかの DB テストは superuser (`postgres`) で繋ぐので RLS を素通りする。ここだけは
//! `common::setup_app_state_as_app_role` で `alc_api_app` として走らせる。行の準備と検証は
//! RLS を素通りする側の pool (`admin`) で行う。
//!
//! 回し方は他の DB integration テストと同じ:
//!   `make db-up && source .test-config && cargo test --test device_dev_axis_test`
//! CI は ci.yml の `bazel-test-db` shard (postgres service + TEST_DATABASE_URL)。

#[macro_use]
mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use uuid::Uuid;

struct Ctx {
    /// RLS を素通りする pool (行の準備と検証用)
    admin: sqlx::PgPool,
    /// `alc_api_app` で繋ぐ、サーバが使う側の pool
    app: sqlx::PgPool,
    base_url: String,
    tenant: Uuid,
    auth: String,
    client: reqwest::Client,
}

/// `alc_api_app` で動くサーバを立てる
async fn setup(tenant_name: &str, max_connections: u32, reset_on_release: bool) -> Ctx {
    let admin_state = common::setup_app_state().await;
    let app_state = common::setup_app_state_as_app_role(max_connections, reset_on_release).await;
    let admin = admin_state.pool().clone();
    let app = app_state.pool().clone();
    let base_url = common::spawn_test_server(app_state).await;
    let tenant = common::create_test_tenant(&admin, tenant_name).await;
    Ctx {
        admin,
        app,
        base_url,
        tenant,
        auth: format!("Bearer {}", common::create_test_jwt(tenant, "admin")),
        client: reqwest::Client::new(),
    }
}

impl Ctx {
    /// tenant の経路の要求を組む。`dev` が Some ならその値で `X-Device-Dev` を付ける
    fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        dev: Option<&str>,
    ) -> reqwest::RequestBuilder {
        let rb = self
            .client
            .request(method, format!("{}{path}", self.base_url))
            .header("Authorization", &self.auth);
        match dev {
            Some(v) => rb.header("X-Device-Dev", v),
            None => rb,
        }
    }

    async fn post(&self, path: &str, dev: Option<&str>, body: Value) -> reqwest::Response {
        self.request(reqwest::Method::POST, path, dev)
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    async fn get(&self, path: &str, dev: Option<&str>) -> reqwest::Response {
        self.request(reqwest::Method::GET, path, dev)
            .send()
            .await
            .unwrap()
    }

    /// 作成の POST を送り、201 を確かめて応答の id を返す
    async fn create(&self, path: &str, dev: Option<&str>, body: Value) -> String {
        let res = self.post(path, dev, body).await;
        let status = res.status();
        let text = res.text().await.unwrap();
        assert_eq!(status, 201, "POST {path} (dev={dev:?}): {text}");
        let json: Value = serde_json::from_str(&text).unwrap();
        json["id"].as_str().unwrap().to_string()
    }

    async fn employee(&self, code: &str) -> String {
        let emp =
            common::create_test_employee(&self.client, &self.base_url, &self.auth, "乗務員", code)
                .await;
        emp["id"].as_str().unwrap().to_string()
    }

    /// 遠隔点呼のセッションを 1 件始める (業務後 = 最初の状態が identity_verified)
    async fn start_session(&self, employee_id: &str, dev: Option<&str>) -> String {
        self.create(
            "/api/tenko/sessions/start",
            dev,
            json!({ "employee_id": employee_id, "tenko_type": "post_operation" }),
        )
        .await
    }

    /// 測定を 1 件保存する (通常点呼の記録の印つき)
    async fn measurement(&self, employee_id: &str, measured_at: &str, dev: Option<&str>) -> String {
        self.create(
            "/api/measurements",
            dev,
            json!({
                "employee_id": employee_id,
                "alcohol_value": 0.0,
                "result_type": "normal",
                "measured_at": measured_at,
                "temperature": 36.5,
                "systolic": 120,
                "diastolic": 80,
                "pulse": 64,
                "record_as_tenko": true,
            }),
        )
        .await
    }

    async fn equipment_failure(&self, dev: Option<&str>) -> String {
        self.create(
            "/api/tenko/equipment-failures",
            dev,
            json!({ "failure_type": "manual_report", "description": "テスト" }),
        )
        .await
    }

    /// 内部の経路 (shared secret) で hub の測定を取り込む
    async fn ingest_hub(&self, dev: Option<&str>, device_id: &str, seq: i64) -> Value {
        let rb = self
            .client
            .post(format!("{}/api/hub/measurements", self.base_url))
            .header(
                "X-Internal-Shared-Secret",
                common::TEST_INTERNAL_SHARED_SECRET,
            )
            .header("X-Tenant-ID", self.tenant.to_string());
        let rb = match dev {
            Some(v) => rb.header("X-Device-Dev", v),
            None => rb,
        };
        let res = rb
            .json(&json!({
                "device_id": device_id,
                "kind": "temperature",
                "seq": seq,
                "recorded_at_ms": 1_752_300_000_000i64,
                "payload": { "type": "temperature", "value": 36.5, "unit": "celsius" }
            }))
            .send()
            .await
            .unwrap();
        let status = res.status();
        let text = res.text().await.unwrap();
        assert_eq!(status, 201, "hub ingest (dev={dev:?}): {text}");
        serde_json::from_str(&text).unwrap()
    }

    /// 行の is_dev を引く (RLS を素通りする pool で)
    async fn is_dev(&self, table: &str, id: &str) -> bool {
        sqlx::query_scalar(&format!("SELECT is_dev FROM alc_api.{table} WHERE id = $1"))
            .bind(Uuid::parse_str(id).unwrap())
            .fetch_one(&self.admin)
            .await
            .unwrap_or_else(|e| panic!("{table} {id}: {e}"))
    }

    /// テナント内の is_dev の並び (false が先)
    async fn is_dev_all(&self, table: &str, where_extra: &str) -> Vec<bool> {
        sqlx::query_scalar(&format!(
            "SELECT is_dev FROM alc_api.{table} WHERE tenant_id = $1 {where_extra} ORDER BY is_dev"
        ))
        .bind(self.tenant)
        .fetch_all(&self.admin)
        .await
        .unwrap()
    }

    /// 一覧の id を取る (`key` は応答の配列の欄)
    async fn list_ids(&self, path: &str, key: &str, dev: Option<&str>) -> Vec<String> {
        let res = self.get(path, dev).await;
        assert_eq!(res.status(), 200, "GET {path} (dev={dev:?})");
        let body: Value = res.json().await.unwrap();
        let mut ids: Vec<String> = body[key]
            .as_array()
            .unwrap_or_else(|| panic!("{path}: {key} が配列でない: {body}"))
            .iter()
            .map(|v| v["id"].as_str().unwrap().to_string())
            .collect();
        ids.sort();
        ids
    }
}

/// 測定に紐づく通常点呼の session / record の is_dev
async fn normal_tenko_is_dev(ctx: &Ctx, measurement_id: &str) -> (Vec<bool>, Vec<bool>) {
    let mid = Uuid::parse_str(measurement_id).unwrap();
    let sessions = sqlx::query_scalar(
        "SELECT is_dev FROM alc_api.tenko_sessions WHERE tenant_id = $1 AND measurement_id = $2",
    )
    .bind(ctx.tenant)
    .bind(mid)
    .fetch_all(&ctx.admin)
    .await
    .unwrap();
    let records = sqlx::query_scalar(
        "SELECT r.is_dev FROM alc_api.tenko_records r
         JOIN alc_api.tenko_sessions s ON s.id = r.session_id
         WHERE r.tenant_id = $1 AND s.measurement_id = $2",
    )
    .bind(ctx.tenant)
    .bind(mid)
    .fetch_all(&ctx.admin)
    .await
    .unwrap();
    (sessions, records)
}

#[tokio::test]
async fn test_tenant_route_writes_follow_the_header() {
    test_group!("dev端末の印 → 書いた行の is_dev (tenant の経路)");
    let ctx = setup("Dev Axis Write", 5, true).await;
    let emp = ctx.employee("DA1").await;

    for (label, dev, expected, measured_at) in [
        ("X-Device-Dev: 1", Some("1"), true, "2026-09-20T01:00:00Z"),
        ("ヘッダなし", None, false, "2026-09-20T02:00:00Z"),
    ] {
        test_case!(&format!("{label} → is_dev = {expected}"), {
            let session = ctx.start_session(&emp, dev).await;
            assert_eq!(
                ctx.is_dev("tenko_sessions", &session).await,
                expected,
                "点呼セッションの開始"
            );

            let m = ctx.measurement(&emp, measured_at, dev).await;
            assert_eq!(ctx.is_dev("measurements", &m).await, expected, "測定の保存");
            assert_eq!(
                normal_tenko_is_dev(&ctx, &m).await,
                (vec![expected], vec![expected]),
                "通常点呼の session と record が 1 組、測定と同じ軸でできる"
            );

            let failure = ctx.equipment_failure(dev).await;
            assert_eq!(
                ctx.is_dev("equipment_failures", &failure).await,
                expected,
                "機器故障"
            );

            let schedule = ctx
                .create(
                    "/api/tenko/schedules",
                    dev,
                    json!({
                        "employee_id": emp,
                        "tenko_type": "post_operation",
                        "responsible_manager_name": "運行管理者",
                        "scheduled_at": "2030-01-01T00:00:00Z",
                    }),
                )
                .await;
            assert_eq!(
                ctx.is_dev("tenko_schedules", &schedule).await,
                expected,
                "点呼の予定"
            );
        });
    }
}

#[tokio::test]
async fn test_only_exactly_one_is_dev() {
    test_group!("X-Device-Dev の値");
    let ctx = setup("Dev Axis Values", 5, true).await;

    for value in ["true", "0", ""] {
        test_case!(&format!("{value:?} は dev にならない"), {
            let failure = ctx.equipment_failure(Some(value)).await;
            assert!(!ctx.is_dev("equipment_failures", &failure).await);
        });
    }
    test_case!("\"1\" だけが dev", {
        let failure = ctx.equipment_failure(Some("1")).await;
        assert!(ctx.is_dev("equipment_failures", &failure).await);
    });
}

#[tokio::test]
async fn test_internal_route_hub_ingest_follows_the_header() {
    test_group!("dev端末の印 → hub の測定の取り込み (内部の経路)");
    let ctx = setup("Dev Axis Hub", 5, true).await;

    test_case!(
        "X-Device-Dev: 1 → is_dev = true、ヘッダなし・\"true\" → false",
        {
            // 3 列 unique (tenant, device_id, seq) がまだ残っているので、軸ごとに連番を分ける
            assert_eq!(ctx.ingest_hub(Some("1"), "hub-dev", 1).await["inserted"], 1);
            assert_eq!(ctx.ingest_hub(None, "hub-dev", 2).await["inserted"], 1);
            assert_eq!(
                ctx.ingest_hub(Some("true"), "hub-dev", 3).await["inserted"],
                1
            );

            let rows: Vec<(i64, bool)> = sqlx::query_as(
            "SELECT seq, is_dev FROM alc_api.hub_measurements WHERE tenant_id = $1 ORDER BY seq",
        )
        .bind(ctx.tenant)
        .fetch_all(&ctx.admin)
        .await
        .unwrap();
            assert_eq!(rows, vec![(1, true), (2, false), (3, false)]);
        }
    );

    test_case!("同じ軸の再送は重複しない (従来どおり)", {
        let again = ctx.ingest_hub(Some("1"), "hub-dev", 1).await;
        assert_eq!(
            (again["inserted"].clone(), again["duplicates"].clone()),
            (json!(0), json!(1))
        );
        let again = ctx.ingest_hub(None, "hub-dev", 2).await;
        assert_eq!(
            (again["inserted"].clone(), again["duplicates"].clone()),
            (json!(0), json!(1))
        );
        assert_eq!(
            ctx.is_dev_all("hub_measurements", "").await,
            vec![false, false, true],
            "行は増えない"
        );
    });
}

#[tokio::test]
async fn test_reads_are_split_by_the_header() {
    test_group!("dev端末の印 → 読み出しの出し分け");
    let ctx = setup("Dev Axis Read", 5, true).await;
    let emp = ctx.employee("DA2").await;

    let dev_session = ctx.start_session(&emp, Some("1")).await;
    let prod_session = ctx.start_session(&emp, None).await;
    let dev_m = ctx
        .measurement(&emp, "2026-09-21T01:00:00Z", Some("1"))
        .await;
    let prod_m = ctx.measurement(&emp, "2026-09-21T02:00:00Z", None).await;

    // 測定の保存が通常点呼の session も 1 件ずつ作るので、軸ごとの session は 2 件
    let session_ids = |dev: bool| {
        let admin = ctx.admin.clone();
        let tenant = ctx.tenant;
        async move {
            let mut ids: Vec<String> = sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM alc_api.tenko_sessions WHERE tenant_id = $1 AND is_dev = $2",
            )
            .bind(tenant)
            .bind(dev)
            .fetch_all(&admin)
            .await
            .unwrap()
            .into_iter()
            .map(|id| id.to_string())
            .collect();
            ids.sort();
            ids
        }
    };
    let dev_sessions = session_ids(true).await;
    let prod_sessions = session_ids(false).await;
    assert_eq!((dev_sessions.len(), prod_sessions.len()), (2, 2));

    test_case!(
        "一覧: dev の要求は dev の行だけ、ヘッダなしは dev の行を見ない",
        {
            assert_eq!(
                ctx.list_ids("/api/tenko/sessions", "sessions", Some("1"))
                    .await,
                dev_sessions
            );
            assert_eq!(
                ctx.list_ids("/api/tenko/sessions", "sessions", None).await,
                prod_sessions
            );
            assert_eq!(
                ctx.list_ids("/api/measurements", "measurements", Some("1"))
                    .await,
                vec![dev_m.clone()]
            );
            assert_eq!(
                ctx.list_ids("/api/measurements", "measurements", None)
                    .await,
                vec![prod_m.clone()]
            );
        }
    );

    test_case!("1 件取得: 軸が違えば 404", {
        for (path, dev, expected) in [
            (format!("/api/tenko/sessions/{dev_session}"), Some("1"), 200),
            (format!("/api/tenko/sessions/{dev_session}"), None, 404),
            (format!("/api/tenko/sessions/{prod_session}"), None, 200),
            (
                format!("/api/tenko/sessions/{prod_session}"),
                Some("1"),
                404,
            ),
            (format!("/api/measurements/{dev_m}"), Some("1"), 200),
            (format!("/api/measurements/{dev_m}"), None, 404),
            (format!("/api/measurements/{prod_m}"), None, 200),
            (format!("/api/measurements/{prod_m}"), Some("1"), 404),
        ] {
            assert_eq!(
                ctx.get(&path, dev).await.status(),
                expected,
                "GET {path} (dev={dev:?})"
            );
        }
    });
}

/// 接続に残っている `app.device_dev` を、サーバと同じ pool から直接読む
async fn device_dev_on_connection(ctx: &Ctx) -> String {
    sqlx::query_scalar("SELECT coalesce(current_setting('app.device_dev', true), '')")
        .fetch_one(&ctx.app)
        .await
        .unwrap()
}

#[tokio::test]
async fn test_connection_reuse_does_not_leak_the_mark() {
    test_group!("接続の使い回し (プール 1 接続)");

    test_case!(
        "after_release なし: 次の要求の set_current_tenant が印を消す",
        {
            let ctx = setup("Dev Axis Reuse A", 1, false).await;
            let dev_failure = ctx.equipment_failure(Some("1")).await;
            assert_eq!(
                device_dev_on_connection(&ctx).await,
                "1",
                "前提: 同じ接続に dev の印が残っている (プールが 1 接続であること)"
            );

            let prod_failure = ctx.equipment_failure(None).await;
            assert!(ctx.is_dev("equipment_failures", &dev_failure).await);
            assert!(
                !ctx.is_dev("equipment_failures", &prod_failure).await,
                "dev の要求の後のヘッダなしの要求は dev にならない"
            );
            assert_eq!(
                ctx.list_ids("/api/tenko/equipment-failures", "failures", None)
                    .await,
                vec![prod_failure.clone()],
                "dev の行が見えない"
            );
            assert_eq!(
                ctx.list_ids("/api/tenko/equipment-failures", "failures", Some("1"))
                    .await,
                vec![dev_failure.clone()]
            );
        }
    );

    test_case!(
        "after_release あり (本番と同じ): 返した接続に印が残らない",
        {
            let ctx = setup("Dev Axis Reuse B", 1, true).await;
            let dev_failure = ctx.equipment_failure(Some("1")).await;
            assert!(ctx.is_dev("equipment_failures", &dev_failure).await);
            assert_eq!(device_dev_on_connection(&ctx).await, "");

            let prod_failure = ctx.equipment_failure(None).await;
            assert!(!ctx.is_dev("equipment_failures", &prod_failure).await);
        }
    );
}

#[tokio::test]
async fn test_public_route_ignores_the_header() {
    test_group!("認証の無い公開 route");
    test_case!(
        "X-Device-Dev: 1 を付けても接続は dev にならない",
        {
            // after_release なしの 1 接続にして、要求の後の接続の状態をそのまま見る
            let ctx = setup("Dev Axis Public", 1, false).await;
            let res = ctx
                .client
                .post(format!("{}/api/devices/register/claim", ctx.base_url))
                .header("X-Device-Dev", "1")
                .json(&json!({
                    "registration_code": "no-such-code",
                    "phone_number": "090-0000-0000",
                    "device_name": "x"
                }))
                .send()
                .await
                .unwrap();
            assert_ne!(res.status(), 401, "公開 route なので認証では弾かれない");
            assert_eq!(device_dev_on_connection(&ctx).await, "");

            // 続くヘッダなしの要求も本番の行になる
            let failure = ctx.equipment_failure(None).await;
            assert!(!ctx.is_dev("equipment_failures", &failure).await);
        }
    );
}

/// 外部への送信を数えるだけの HTTP クライアント
#[derive(Default)]
struct CountingHttp {
    calls: AtomicUsize,
}

#[async_trait::async_trait]
impl rust_alc_api::webhook::WebhookHttpClient for CountingHttp {
    async fn deliver(
        &self,
        _url: &str,
        _event_type: &str,
        _payload: &Value,
        _secret: Option<&str>,
    ) -> Result<(Option<i32>, Option<String>, bool), anyhow::Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok((Some(200), Some("ok".to_string()), true))
    }
}

#[tokio::test]
async fn test_webhook_is_not_fired_for_dev_requests() {
    test_group!("webhook (アルコール検知)");

    let admin_state = common::setup_app_state().await;
    let mut app_state = common::setup_app_state_as_app_role(5, true).await;
    let admin = admin_state.pool().clone();
    let app = app_state.pool().clone();
    let http = Arc::new(CountingHttp::default());
    app_state.webhook = Some(Arc::new(rust_alc_api::webhook::PgWebhookService::new(
        Arc::new(rust_alc_api::db::repository::PgWebhookRepository::new(
            app.clone(),
        )),
        http.clone(),
    )));
    let base_url = common::spawn_test_server(app_state).await;
    let tenant = common::create_test_tenant(&admin, "Dev Axis Webhook").await;
    let ctx = Ctx {
        admin,
        app,
        base_url,
        tenant,
        auth: format!("Bearer {}", common::create_test_jwt(tenant, "admin")),
        client: reqwest::Client::new(),
    };
    sqlx::query(
        "INSERT INTO alc_api.webhook_configs (tenant_id, event_type, url, enabled)
         VALUES ($1, 'alcohol_detected', 'https://example.com/hook', TRUE)",
    )
    .bind(ctx.tenant)
    .execute(&ctx.admin)
    .await
    .unwrap();
    let emp = ctx.employee("DA3").await;

    // アルコール検知 (fail) を出す。webhook は handler が裏で発火する
    let detect = |dev: Option<&'static str>| {
        let ctx = &ctx;
        let emp = emp.clone();
        async move {
            let session = ctx.start_session(&emp, dev).await;
            let res = ctx
                .request(
                    reqwest::Method::PUT,
                    &format!("/api/tenko/sessions/{session}/alcohol"),
                    dev,
                )
                .json(&json!({ "alcohol_result": "fail", "alcohol_value": 0.25 }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200, "alcohol (dev={dev:?})");
        }
    };

    test_case!(
        "dev の要求では配信の行が増えず、外部へも送らない",
        {
            detect(Some("1")).await;

            // 続けてヘッダなしで出し、こちらの配信が記録されるまで待つ。
            // dev の分が (誤って) 発火していれば、先に走っているのでここまでに数に出る
            detect(None).await;
            for _ in 0..100 {
                if !ctx.is_dev_all("webhook_deliveries", "").await.is_empty() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
            let deliveries = ctx.is_dev_all("webhook_deliveries", "").await;

            assert_eq!(
                deliveries,
                vec![false],
                "配信の行はヘッダなしの 1 件だけ (本番の行)"
            );
            assert_eq!(http.calls.load(Ordering::SeqCst), 1, "外部への送信も 1 回");
        }
    );
}

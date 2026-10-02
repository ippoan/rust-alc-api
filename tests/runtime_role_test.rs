//! backend を「表の所有者でない実行用ロール」`alc_api_rt` で繋いだときに、認証前・テナント横断の
//! 経路が通ることを実 DB で固定する (alc-migrations 158)。
//!
//! Refs ippoan/alc-app#387
//!
//! 本番の backend は表の所有者で繋いでいるあいだ、`FORCE ROW LEVEL SECURITY` の無い表に RLS が
//! 掛からない。所有者でないロールへ切り替えるとポリシーが掛かり始め、tenant context を立てずに
//! 打つ query は落ちる (users は行に当たるとエラー、招待は 0 行)。ログイン・端末の登録・着信の通知・
//! 超過検知・参加の申請は「テナントが決まる前」か「全テナント横断」が本来の動きなので、
//! SECURITY DEFINER 関数 (158) 経由で打つ。
//!
//! ほかの DB テストは superuser (`postgres`) で繋ぐので RLS を素通りする。ここは
//! `common::setup_app_state_as_app_role` (接続ごとに `SET ROLE alc_api_rt`) で走らせる。
//! テスト DB の表の所有者は postgres なので、`alc_api_rt` は本番の切り替え後と同じ「非所有者」。
//! 行の準備と検証は RLS を素通りする側の pool (`admin`) で行う。
//!
//! mock (`tests/mock_helpers/`) は SQL を 1 文字も通らないので、実 DB でしか縛れない。
//! CI は ci.yml の `bazel-test-db` shard (`db-runtime-role`)。

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use chrono::{Duration, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

use alc_core::tenant::TenantConn;
use rust_alc_api::db::repository::{
    AuthRepository, DtakoUploadRepository, PgAuthRepository, PgDtakoUploadRepository,
    PgTenantUsersRepository, PgTenkoOverdueRepository, PgWebhookRepository, TenantUsersRepository,
};

const FCM_SECRET: &str = "runtime-role-fcm-secret";

struct Ctx {
    /// RLS を素通りする pool (行の準備と検証用)
    admin: sqlx::PgPool,
    /// `alc_api_rt` で繋ぐ、サーバが使う側の pool
    app: sqlx::PgPool,
    base_url: String,
    fcm: Arc<common::MockFcmSender>,
    client: reqwest::Client,
    /// 実行用ロールの属性・表の FORCE を一時的に変えるテスト (rls-check の陽性対照) と重ならないための共有ロック
    _role_attrs: Option<tokio::sync::RwLockReadGuard<'static, ()>>,
}

/// 実行用ロール `alc_api_rt` の属性や表の FORCE を変えるテストは write、それ以外は read で取る。
/// どちらの変更も DB 全体に効く (ほかの接続の RLS が外れる・rls-check の合否が変わる) ので、
/// 同じ binary の中で直列にする。
static ROLE_ATTRS: tokio::sync::RwLock<()> = tokio::sync::RwLock::const_new(());

/// `alc_api_rt` で動くサーバを立てる (FCM は送信を記録するだけの mock)
async fn setup() -> Ctx {
    let guard = ROLE_ATTRS.read().await;
    let mut ctx = setup_unlocked().await;
    ctx._role_attrs = Some(guard);
    ctx
}

/// [`setup`] の本体。`ROLE_ATTRS` を自分で取っている呼び出し元だけが直接呼ぶ。
async fn setup_unlocked() -> Ctx {
    let admin_state = common::setup_app_state().await;
    let mut app_state = common::setup_app_state_as_app_role(5, true).await;
    let fcm = Arc::new(common::MockFcmSender::new());
    app_state.fcm = Some(fcm.clone());
    let admin = admin_state.pool().clone();
    let app = app_state.pool().clone();
    let base_url = common::spawn_test_server(app_state).await;
    Ctx {
        admin,
        app,
        base_url,
        fcm,
        client: reqwest::Client::new(),
        _role_attrs: None,
    }
}

fn unique(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::new_v4().simple())
}

/// 内部用 secret を立てる。この binary の全テストが同じ値を使い、消さない
/// (立てている間に別のテストが読んでも同じ値)。
fn set_fcm_secret() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| std::env::set_var("FCM_INTERNAL_SECRET", FCM_SECRET));
}

impl Ctx {
    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    /// auth-worker の内部口 (`require_internal_jwt` 配下) への POST
    async fn internal_post(&self, path: &str, body: Value) -> reqwest::Response {
        self.client
            .post(self.url(path))
            .header(
                "Authorization",
                format!("Bearer {}", common::create_test_internal_jwt()),
            )
            .json(&body)
            .send()
            .await
            .unwrap()
    }

    /// 内部用の口 `GET /api/internal/rls-check` を叩く (`query` は `?a=b` の形か空、`bearer` は無ければ付けない)
    async fn rls_check(&self, query: &str, bearer: Option<&str>) -> reqwest::Result<(u16, String)> {
        let mut req = self
            .client
            .get(self.url(&format!("/api/internal/rls-check{query}")));
        if let Some(token) = bearer {
            req = req.header("Authorization", format!("Bearer {token}"));
        }
        let res = req.send().await?;
        let status = res.status().as_u16();
        Ok((status, res.text().await?))
    }

    /// 内部用の認可を通して rls-check を叩き、200 の JSON を返す
    async fn rls_check_ok(&self, query: &str) -> Value {
        let (status, text) = self
            .rls_check(query, Some(&common::create_test_internal_jwt()))
            .await
            .unwrap();
        assert_eq!(status, 200, "GET /api/internal/rls-check{query}: {text}");
        serde_json::from_str(&text).unwrap()
    }

    /// 内部口で user を解決 / 作成し、応答の JSON を返す
    async fn upsert_user(&self, path: &str, body: Value) -> Value {
        let res = self.internal_post(path, body).await;
        let status = res.status();
        let text = res.text().await.unwrap();
        assert_eq!(status, 200, "POST {path}: {text}");
        serde_json::from_str(&text).unwrap()
    }

    /// ログインの最後の手 (refresh token のハッシュの保存) を打ち、DB に入った値を確かめる
    async fn save_refresh_token(&self, user_id: &str) {
        let hash = unique("hash");
        let res = self
            .internal_post(
                "/api/internal/auth/refresh-token",
                json!({
                    "user_id": user_id,
                    "refresh_hash": hash,
                    "expires_at": Utc::now() + Duration::days(30),
                }),
            )
            .await;
        assert_eq!(res.status(), 204, "refresh-token (user {user_id})");

        let (stored, in_future): (Option<String>, Option<bool>) = sqlx::query_as(
            "SELECT refresh_token_hash, refresh_token_expires_at > NOW() \
             FROM alc_api.users WHERE id = $1::uuid",
        )
        .bind(user_id)
        .fetch_one(&self.admin)
        .await
        .unwrap();
        assert_eq!(stored.as_deref(), Some(hash.as_str()));
        assert_eq!(in_future, Some(true), "期限は引数の expires_at が入る");
    }

    /// 管理者の token を持つ user を 1 人作る (端末の承認者は users への FK)
    async fn admin_user(&self, tenant: Uuid) -> String {
        let user = PgAuthRepository::new(self.admin.clone())
            .create_user_line(tenant, &unique("admin-line"), "Admin")
            .await
            .expect("create_user_line failed");
        format!(
            "Bearer {}",
            common::create_test_jwt_for_user(user.id, tenant, "admin@example.test", "admin")
        )
    }

    async fn insert_device(&self, tenant: Uuid, fcm_token: &str, is_dev: bool) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO alc_api.devices \
                (tenant_id, device_name, device_type, status, fcm_token, call_enabled, is_dev_device) \
             VALUES ($1, 'Runtime Role Device', 'android', 'active', $2, TRUE, $3) RETURNING id",
        )
        .bind(tenant)
        .bind(fcm_token)
        .bind(is_dev)
        .fetch_one(&self.admin)
        .await
        .expect("device insert failed")
    }

    fn fcm_tokens_sent(&self) -> Vec<String> {
        self.fcm
            .sent
            .lock()
            .unwrap()
            .iter()
            .map(|(token, _)| token.clone())
            .collect()
    }
}

// ============================================================
// 7. 陰性: このテストは RLS が掛かる立場で走っている
// ============================================================

/// tenant を立てずに `users` を直接読むとエラーになり、別テナントを立てると見えない。
/// (= サーバ側の pool は所有者でも superuser でもない。ここが通らなければ、
/// 下のテストは RLS を素通りしていて何も証明しない)
#[tokio::test]
async fn runtime_role_is_subject_to_rls() {
    let ctx = setup().await;
    let tenant_a = common::create_test_tenant(&ctx.admin, "RT RLS A").await;
    let tenant_b = common::create_test_tenant(&ctx.admin, "RT RLS B").await;
    let user = PgAuthRepository::new(ctx.admin.clone())
        .create_user_line(tenant_a, &unique("rls-line"), "RLS User")
        .await
        .expect("create_user_line failed");

    let role: String = sqlx::query_scalar("SELECT current_user::text")
        .fetch_one(&ctx.app)
        .await
        .unwrap();
    assert_eq!(role, "alc_api_rt");

    let direct = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM alc_api.users WHERE id = $1")
        .bind(user.id)
        .fetch_one(&ctx.app)
        .await;
    assert!(
        direct.is_err(),
        "tenant を立てずに users を読めた ({direct:?}) = RLS が掛かっていない"
    );

    for (tenant, expected) in [(tenant_b, 0i64), (tenant_a, 1i64)] {
        let mut tc = TenantConn::acquire(&ctx.app, &tenant.to_string())
            .await
            .unwrap();
        let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM alc_api.users WHERE id = $1")
            .bind(user.id)
            .fetch_one(&mut *tc.conn)
            .await
            .unwrap();
        assert_eq!(visible, expected, "tenant {tenant} から見える行数");
    }

    // 招待は宛先を絞ったポリシー (158 で alc_api_rt を足した)。tenant 無しだと 0 行で、エラーにならない
    let email = format!("{}@example.test", unique("rls-invite"));
    PgTenantUsersRepository::new(ctx.admin.clone())
        .invite_user(tenant_a, &email, "viewer")
        .await
        .expect("invite_user failed");
    let invisible: i64 =
        sqlx::query_scalar("SELECT count(*) FROM alc_api.tenant_allowed_emails WHERE email = $1")
            .bind(&email)
            .fetch_one(&ctx.app)
            .await
            .expect("tenant_allowed_emails は tenant 無しでもエラーにならない");
    assert_eq!(invisible, 0);
}

// ============================================================
// 1. ログイン 3 経路 / 2. 招待
// ============================================================

/// Google: 招待された email の初回ログイン (招待の逆引き → user 作成 → 招待の消費) と、
/// 2 回目 (google_sub の逆引き)。どちらも最後に refresh token を保存する。
#[tokio::test]
async fn google_login_new_user_via_invitation_then_existing() {
    let ctx = setup().await;
    let tenant = common::create_test_tenant(&ctx.admin, "RT Google").await;
    let email = format!("{}@example.test", unique("google"));
    let google_sub = unique("gsub");

    // 招待は実行用ロールの接続で作る (tenant_users の管理画面と同じ repo)
    let invitation = PgTenantUsersRepository::new(ctx.app.clone())
        .invite_user(tenant, &email, "viewer")
        .await
        .expect("invite_user failed");
    assert_eq!(invitation.tenant_id, tenant);

    let body = json!({ "google_sub": google_sub, "email": email, "name": "Google User" });
    let created = ctx
        .upsert_user("/api/internal/auth/users/upsert-google", body.clone())
        .await;
    assert_eq!(created["tenant_id"], tenant.to_string());
    assert_eq!(created["role"], "viewer", "招待の role で作られる");
    assert_eq!(created["google_sub"], google_sub);
    let user_id = created["id"].as_str().unwrap().to_string();

    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*) FROM alc_api.tenant_allowed_emails WHERE email = $1")
            .bind(&email)
            .fetch_one(&ctx.admin)
            .await
            .unwrap();
    assert_eq!(remaining, 0, "招待は使い済みで消える");
    ctx.save_refresh_token(&user_id).await;

    // 2 回目: 既存の行がそのまま返る (行は増えない)
    let found = ctx
        .upsert_user("/api/internal/auth/users/upsert-google", body)
        .await;
    assert_eq!(found["id"], user_id);
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM alc_api.users WHERE google_sub = $1")
        .bind(&google_sub)
        .fetch_one(&ctx.admin)
        .await
        .unwrap();
    assert_eq!(rows, 1);
    ctx.save_refresh_token(&user_id).await;
}

/// 招待もテナントの email_domain の一致も無い Google ログインは 403 (招待の逆引きが 0 行で返る)
#[tokio::test]
async fn google_login_without_invitation_is_rejected() {
    let ctx = setup().await;
    let res = ctx
        .internal_post(
            "/api/internal/auth/users/upsert-google",
            json!({
                "google_sub": unique("gsub"),
                "email": format!("nobody@{}.example.test", unique("domain")),
                "name": "Nobody",
            }),
        )
        .await;
    assert_eq!(res.status(), 403);
}

/// LINE WORKS: 新規 (逆引きが外れる → 作成) と既存 (逆引きが当たる)
#[tokio::test]
async fn lineworks_login_new_then_existing() {
    let ctx = setup().await;
    let tenant = common::create_test_tenant(&ctx.admin, "RT LINE WORKS").await;
    let body = json!({
        "tenant_id": tenant,
        "lineworks_id": unique("lw"),
        "email": format!("{}@example.test", unique("lw")),
        "name": "LW User",
    });

    let created = ctx
        .upsert_user("/api/internal/auth/users/upsert-lineworks", body.clone())
        .await;
    assert_eq!(created["tenant_id"], tenant.to_string());
    assert_eq!(created["role"], "viewer");
    let user_id = created["id"].as_str().unwrap().to_string();
    ctx.save_refresh_token(&user_id).await;

    let found = ctx
        .upsert_user("/api/internal/auth/users/upsert-lineworks", body)
        .await;
    assert_eq!(found["id"], user_id);
    ctx.save_refresh_token(&user_id).await;
}

/// LINE: 新規 (逆引きが外れる → 作成) と既存 (逆引きが当たる)
#[tokio::test]
async fn line_login_new_then_existing() {
    let ctx = setup().await;
    let tenant = common::create_test_tenant(&ctx.admin, "RT LINE").await;
    let body = json!({
        "tenant_id": tenant,
        "line_user_id": unique("line"),
        "name": "LINE User",
    });

    let created = ctx
        .upsert_user("/api/internal/auth/users/upsert-line", body.clone())
        .await;
    assert_eq!(created["tenant_id"], tenant.to_string());
    assert_eq!(created["role"], "viewer");
    let user_id = created["id"].as_str().unwrap().to_string();
    ctx.save_refresh_token(&user_id).await;

    let found = ctx
        .upsert_user("/api/internal/auth/users/upsert-line", body)
        .await;
    assert_eq!(found["id"], user_id);
    ctx.save_refresh_token(&user_id).await;
}

/// refresh token の保存は、渡した user の行だけを書き換える (関数の引数の順の取り違えの検出)
#[tokio::test]
async fn save_refresh_token_touches_only_the_given_user() {
    let ctx = setup().await;
    let tenant = common::create_test_tenant(&ctx.admin, "RT Refresh").await;
    let auth = PgAuthRepository::new(ctx.admin.clone());
    let target = auth
        .create_user_line(tenant, &unique("refresh-a"), "Target")
        .await
        .unwrap();
    let other = auth
        .create_user_line(tenant, &unique("refresh-b"), "Other")
        .await
        .unwrap();

    ctx.save_refresh_token(&target.id.to_string()).await;

    let untouched: Option<String> =
        sqlx::query_scalar("SELECT refresh_token_hash FROM alc_api.users WHERE id = $1")
            .bind(other.id)
            .fetch_one(&ctx.admin)
            .await
            .unwrap();
    assert_eq!(untouched, None);
}

// ============================================================
// 3. 端末: 登録要求 → 承認 → 承認後のポーリング → tenant の逆引き
// ============================================================

#[tokio::test]
async fn device_registration_status_after_approval_and_tenant_lookup() {
    let ctx = setup().await;
    let tenant = common::create_test_tenant(&ctx.admin, "RT Device").await;
    let admin_auth = ctx.admin_user(tenant).await;

    // 端末が登録要求を出す (公開、tenant 無し)
    let res = ctx
        .client
        .post(ctx.url("/api/devices/register/request"))
        .json(&json!({ "device_name": "RT Kiosk" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    let code = body["registration_code"].as_str().unwrap().to_string();

    // 承認前のポーリング: pending で、端末の情報は返さない
    let status_url = ctx.url(&format!("/api/devices/register/status/{code}"));
    let res = ctx.client.get(&status_url).send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["status"], "pending");
    assert!(body.get("device_id").is_none());
    assert!(body.get("settings_token").is_none());

    // 管理者が承認する
    let res = ctx
        .client
        .post(ctx.url(&format!("/api/devices/approve-by-code/{code}")))
        .header("Authorization", &admin_auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    let device_id = body["device_id"].as_str().unwrap().to_string();
    assert_eq!(body["tenant_id"], tenant.to_string());

    // 承認後のポーリング (tenant 無しのまま)。devices の行にしか無い settings_token まで返る
    let res = ctx.client.get(&status_url).send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["status"], "approved");
    assert_eq!(body["device_id"], device_id);
    assert_eq!(body["tenant_id"], tenant.to_string());
    assert_eq!(body["device_name"], "RT Kiosk");
    let stored_token: Option<Uuid> =
        sqlx::query_scalar("SELECT settings_token FROM alc_api.devices WHERE id = $1::uuid")
            .bind(&device_id)
            .fetch_one(&ctx.admin)
            .await
            .unwrap();
    let stored_token = stored_token.expect("承認で settings_token が発行される");
    assert_eq!(body["settings_token"], stored_token.to_string());

    // 存在しないコードは 404
    let res = ctx
        .client
        .get(ctx.url(&format!("/api/devices/register/status/{}", Uuid::new_v4())))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);

    // device_id から tenant を引く 2 つの口 (内部口と、端末が叩く公開口)
    let pairing_url = ctx.url(&format!("/api/internal/devices/{device_id}/pairing-tenant"));
    let internal_auth = format!("Bearer {}", common::create_test_internal_jwt());
    let res = ctx
        .client
        .get(&pairing_url)
        .header("Authorization", &internal_auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["tenant_id"], tenant.to_string());

    let other_token = unique("fcm-other");
    ctx.insert_device(tenant, &other_token, false).await;
    let dismiss = || async {
        ctx.client
            .post(ctx.url("/api/devices/fcm-dismiss-test"))
            .json(&json!({ "device_id": device_id }))
            .send()
            .await
            .unwrap()
    };
    let res = dismiss().await;
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["sent"], 1, "同じテナントの他の端末 1 台へ送る");
    assert!(ctx.fcm_tokens_sent().contains(&other_token));

    // 無効にした端末は、どちらの口でも「無い」扱い (status = 'active' だけを返す)
    let res = ctx
        .client
        .post(ctx.url(&format!("/api/devices/disable/{device_id}")))
        .header("Authorization", &admin_auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
    let res = ctx
        .client
        .get(&pairing_url)
        .header("Authorization", &internal_auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
    assert_eq!(dismiss().await.status(), 404);

    // 登録の無い device_id も 404
    let res = ctx
        .client
        .get(ctx.url(&format!(
            "/api/internal/devices/{}/pairing-tenant",
            Uuid::new_v4()
        )))
        .header("Authorization", &internal_auth)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// ============================================================
// 4. 全テナント横断の端末の列挙 (内部用 secret 付き)
// ============================================================

#[tokio::test]
async fn cross_tenant_device_listings_work_with_internal_secret() {
    set_fcm_secret();
    let ctx = setup().await;
    let tenant_a = common::create_test_tenant(&ctx.admin, "RT FCM A").await;
    let tenant_b = common::create_test_tenant(&ctx.admin, "RT FCM B").await;
    let token_a = unique("fcm-a");
    let token_b = unique("fcm-b");
    let device_a = ctx.insert_device(tenant_a, &token_a, true).await;
    let device_b = ctx.insert_device(tenant_b, &token_b, false).await;

    // 着信の通知 (list_fcm_devices): 両テナントの端末へ送る
    let res = ctx
        .client
        .post(ctx.url("/api/devices/fcm-notify-call"))
        .header("X-Internal-Secret", FCM_SECRET)
        .json(&json!({ "room_ids": ["room-1"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let sent = ctx.fcm_tokens_sent();
    assert!(sent.contains(&token_a) && sent.contains(&token_b));

    // テスト送信 (list_all_callable_devices): 除外した端末以外の全テナントの端末
    let res = ctx
        .client
        .post(ctx.url("/api/devices/test-fcm-all-exclude"))
        .header("X-Internal-Secret", FCM_SECRET)
        .json(&json!({ "exclude_device_ids": [device_b.to_string()] }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    let ids: Vec<&str> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["device_id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&device_a.to_string().as_str()));
    assert!(!ids.contains(&device_b.to_string().as_str()));
    let row = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["device_id"] == device_a.to_string())
        .unwrap();
    assert_eq!(row["device_name"], "Runtime Role Device");

    // 開発用の端末への OTA (list_dev_device_tenant_ids → テナントごとの一覧):
    // dev の端末だけに届く
    let res = ctx
        .client
        .post(ctx.url("/api/devices/trigger-update-dev"))
        .header("X-Internal-Secret", FCM_SECRET)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    let ids: Vec<&str> = body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["device_id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&device_a.to_string().as_str()));
    assert!(!ids.contains(&device_b.to_string().as_str()));

    // secret が違えば 401 (認可は変えていない)
    let res = ctx
        .client
        .post(ctx.url("/api/devices/test-fcm-all-exclude"))
        .header("X-Internal-Secret", "wrong")
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

// ============================================================
// 5. 超過検知の 1 tick
// ============================================================

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

/// 全テナントの設定の列挙 → 超過した予定 → 乗務員の名前 → 通知済みの印 → 配信の記録、の 1 周
#[tokio::test]
async fn overdue_tick_marks_and_delivers() {
    let ctx = setup().await;
    let tenant = common::create_test_tenant(&ctx.admin, "RT Overdue").await;
    sqlx::query(
        "INSERT INTO alc_api.webhook_configs (tenant_id, event_type, url, enabled)
         VALUES ($1, 'tenko_overdue', 'https://example.com/hook', TRUE)",
    )
    .bind(tenant)
    .execute(&ctx.admin)
    .await
    .unwrap();
    let employee: Uuid = sqlx::query_scalar(
        "INSERT INTO alc_api.employees (tenant_id, nfc_id, name) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant)
    .bind(Uuid::new_v4().to_string())
    .bind("RT Driver")
    .fetch_one(&ctx.admin)
    .await
    .unwrap();
    let schedule: Uuid = sqlx::query_scalar(
        "INSERT INTO alc_api.tenko_schedules
            (tenant_id, employee_id, tenko_type, responsible_manager_name, scheduled_at, instruction)
         VALUES ($1, $2, 'pre_operation', 'Manager', NOW() - INTERVAL '2 hours', 'note')
         RETURNING id",
    )
    .bind(tenant)
    .bind(employee)
    .fetch_one(&ctx.admin)
    .await
    .unwrap();

    let http = CountingHttp::default();
    rust_alc_api::webhook::check_overdue_schedules(
        &PgWebhookRepository::new(ctx.app.clone()),
        &PgTenkoOverdueRepository::new(ctx.app.clone()),
        &http,
    )
    .await
    .expect("check_overdue_schedules failed");

    let notified: bool = sqlx::query_scalar(
        "SELECT overdue_notified_at IS NOT NULL FROM alc_api.tenko_schedules WHERE id = $1",
    )
    .bind(schedule)
    .fetch_one(&ctx.admin)
    .await
    .unwrap();
    assert!(notified, "超過した予定に通知済みの印が付く");
    assert!(http.calls.load(Ordering::SeqCst) >= 1);

    let (deliveries, employee_name): (i64, Option<String>) = sqlx::query_as(
        "SELECT count(*), max(payload->'data'->>'employee_name') \
         FROM alc_api.webhook_deliveries \
         WHERE tenant_id = $1 AND event_type = 'tenko_overdue' AND success",
    )
    .bind(tenant)
    .fetch_one(&ctx.admin)
    .await
    .unwrap();
    assert_eq!(deliveries, 1);
    assert_eq!(employee_name.as_deref(), Some("RT Driver"));
}

// ============================================================
// 6. 参加の申請: 作成 (申請先は他テナント) → 一覧 → 承認 / 却下
// ============================================================

#[tokio::test]
async fn access_request_create_list_approve_decline() {
    let ctx = setup().await;
    let target = common::create_test_tenant(&ctx.admin, "RT Access Target").await;
    let home = common::create_test_tenant(&ctx.admin, "RT Access Home").await;
    let slug: String = sqlx::query_scalar("SELECT slug FROM alc_api.tenants WHERE id = $1")
        .bind(target)
        .fetch_one(&ctx.admin)
        .await
        .unwrap();
    let auth = PgAuthRepository::new(ctx.admin.clone());

    // 申請者は自分のテナント (home) の token で、他テナント (target) へ申請する
    let mut request_ids = Vec::new();
    for label in ["approve", "decline"] {
        let applicant = auth
            .create_user_line(home, &unique(label), "Applicant")
            .await
            .unwrap();
        let token = common::create_test_jwt_for_user(
            applicant.id,
            home,
            "applicant@example.test",
            "viewer",
        );
        let res = ctx
            .client
            .post(ctx.url("/api/access-requests"))
            .header("Authorization", format!("Bearer {token}"))
            .json(&json!({ "org_slug": slug }))
            .send()
            .await
            .unwrap();
        let status = res.status();
        let text = res.text().await.unwrap();
        assert_eq!(status, 201, "POST /api/access-requests: {text}");
        let body: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(body["tenant_id"], target.to_string());
        assert_eq!(body["user_id"], applicant.id.to_string());
        assert_eq!(body["status"], "pending");
        assert_eq!(body["org_name"], "RT Access Target");
        request_ids.push(body["id"].as_str().unwrap().to_string());
    }

    let admin = format!("Bearer {}", common::create_test_jwt(target, "admin"));
    let list = |query: &'static str| {
        let ctx = &ctx;
        let admin = admin.clone();
        async move {
            let res = ctx
                .client
                .get(ctx.url(&format!("/api/access-requests{query}")))
                .header("Authorization", admin)
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
            let body: Value = res.json().await.unwrap();
            body["requests"].as_array().unwrap().len()
        }
    };
    assert_eq!(list("").await, 2);

    // 申請者のテナントの管理者には見えない
    let res = ctx
        .client
        .get(ctx.url("/api/access-requests"))
        .header(
            "Authorization",
            format!("Bearer {}", common::create_test_jwt(home, "admin")),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: Value = res.json().await.unwrap();
    assert_eq!(body["requests"].as_array().unwrap().len(), 0);

    let res = ctx
        .client
        .post(ctx.url(&format!("/api/access-requests/{}/approve", request_ids[0])))
        .header("Authorization", &admin)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
    let res = ctx
        .client
        .post(ctx.url(&format!("/api/access-requests/{}/decline", request_ids[1])))
        .header("Authorization", &admin)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
    assert_eq!(list("?status=pending").await, 0);
    assert_eq!(list("?status=approved").await, 1);
    assert_eq!(list("?status=declined").await, 1);
}

// ============================================================
// 8. 内部用の口 rls-check (Refs ippoan/auth-worker#605)
// ============================================================

fn sorted_keys(value: &Value) -> Vec<String> {
    let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}

/// 繋いでいるロールの数は、同じ DB を使うほかの接続で変わりうるので比べない
fn without_connections(mut body: Value) -> Value {
    let map = body.as_object_mut().unwrap();
    map.remove("connections");
    map.remove("owner_role_connected");
    body
}

/// 実行用ロールの接続 + 内部用の認可で、検査が全部通る。応答の key は契約ちょうど。
#[tokio::test]
async fn rls_check_passes_for_runtime_role() {
    let ctx = setup().await;
    let body = ctx.rls_check_ok("").await;

    assert_eq!(body["runtime_role"]["current_user"], "alc_api_rt");
    assert_eq!(body["runtime_role"]["is_runtime_role"], true);
    for attr in [
        "rolsuper",
        "rolbypassrls",
        "rolinherit",
        "member_of_table_owner",
    ] {
        assert_eq!(body["runtime_role"][attr], false, "{attr}: {body}");
    }
    assert_eq!(body["invariants"]["violation_count"], 0, "{body}");
    assert_eq!(body["invariants"]["violations"], json!([]));

    // 違反が無くても、流した検査が全部 (crate の一覧の番号の順 = 0〜8) 0 件で並ぶ
    let checks = body["invariants"]["checks"].as_array().unwrap();
    let numbers: Vec<i64> = checks
        .iter()
        .map(|c| c["check_no"].as_i64().unwrap())
        .collect();
    let listed: Vec<i64> = alc_migrations::RLS_INVARIANT_CHECKS
        .iter()
        .map(|&(check_no, _)| i64::from(check_no))
        .collect();
    assert_eq!(numbers, listed, "{body}");
    assert_eq!(numbers.first(), Some(&0));
    assert_eq!(
        numbers.last(),
        Some(&8),
        "検査 6・7・8 が並んでいない: {body}"
    );
    for check in checks {
        assert_eq!(sorted_keys(check), ["check_no", "title", "violations"]);
        assert!(!check["title"].as_str().unwrap().is_empty());
        assert_eq!(check["violations"], 0, "{check}");
    }

    // 状態 (カタログの実物): 最上位の key は 6 個で、組ごとの表の数の合計が表の総数と合う
    let state = &body["state"];
    assert!(state.is_object(), "state が取れていない: {body}");
    assert_eq!(
        sorted_keys(state),
        [
            "policy_names",
            "security_definer_functions",
            "sequences",
            "table_count",
            "tables",
            "views"
        ]
    );
    let table_count = state["table_count"].as_i64().unwrap();
    assert!(table_count > 0);
    let grouped: i64 = state["tables"]
        .as_array()
        .unwrap()
        .iter()
        .map(|group| group["count"].as_i64().unwrap())
        .sum();
    assert_eq!(grouped, table_count);
    assert_eq!(
        state["policy_names"].as_object().unwrap().len() as i64,
        table_count
    );

    // 履歴との食い違い: テスト DB は全 migration を 0 から当てた状態なので、期待する状態と一致する
    // (所有者のロール名は比べない)
    let drift = &body["drift"];
    assert_eq!(drift["matches_expected"], true, "{drift}");
    assert_eq!(drift["tables"], json!([]), "{drift}");
    assert_eq!(drift["functions"], json!([]), "{drift}");
    assert!(drift["views"].is_null(), "{drift}");
    assert!(drift["sequences"].is_null(), "{drift}");
    assert_eq!(drift["unreadable"], json!([]), "{drift}");
    assert_eq!(
        sorted_keys(drift),
        [
            "functions",
            "matches_expected",
            "sequences",
            "tables",
            "unreadable",
            "views"
        ]
    );

    // 合否の内訳は 4 つちょうどで、全部 true
    assert_eq!(
        body["verdicts"],
        json!({ "invariants": true, "runtime_role": true, "migrations": true, "drift": true }),
        "{body}"
    );
    assert_eq!(body["migrations"]["matches_binary"], true, "{body}");
    assert!(body["migrations"]["applied"].as_i64().unwrap() > 0);
    assert_eq!(
        body["migrations"]["applied"],
        body["migrations"]["binary_count"]
    );
    assert_eq!(
        body["migrations"]["max_version"],
        body["migrations"]["binary_max_version"]
    );
    assert_eq!(body["ok"], true, "{body}");

    // テストの pool は接続後に SET ROLE するので、pg_stat_activity の usename はログインロールになり、
    // 実行用ロールの行は出ない。ここでは形だけを見る (要素の key は src 側の単体テストで固定)。
    for conn in body["connections"].as_array().expect("connections は配列") {
        assert_eq!(sorted_keys(conn), ["count", "usename"]);
    }
    assert!(body["owner_role_connected"].is_boolean());

    assert_eq!(
        sorted_keys(&body),
        [
            "connections",
            "drift",
            "invariants",
            "migrations",
            "ok",
            "owner_role_connected",
            "runtime_role",
            "state",
            "verdicts"
        ]
    );
    assert_eq!(
        sorted_keys(&body["migrations"]),
        [
            "applied",
            "binary_count",
            "binary_max_version",
            "matches_binary",
            "max_version"
        ]
    );
    assert_eq!(
        sorted_keys(&body["runtime_role"]),
        [
            "current_user",
            "is_runtime_role",
            "member_of_table_owner",
            "rolbypassrls",
            "rolinherit",
            "rolsuper"
        ]
    );
    assert_eq!(
        sorted_keys(&body["invariants"]),
        ["checks", "violation_count", "violations"]
    );
}

/// 内部用の認可が無ければ 401 (無認証の口ではない)
#[tokio::test]
async fn rls_check_requires_internal_auth() {
    let ctx = setup().await;
    for bearer in [None, Some("not-a-valid-token")] {
        let (status, text) = ctx.rls_check("", bearer).await.unwrap();
        assert_eq!(status, 401, "bearer {bearer:?}: {text}");
        assert!(
            !text.contains("runtime_role"),
            "401 の本文に検査の結果が出ている: {text}"
        );
    }
}

/// 口は引数を読まない: query を付けても応答は同じ
#[tokio::test]
async fn rls_check_ignores_query_parameters() {
    let ctx = setup().await;
    let plain = ctx.rls_check_ok("").await;
    let with_query = ctx.rls_check_ok("?sql=select+1&table=users").await;
    assert_eq!(
        without_connections(with_query),
        without_connections(plain.clone())
    );
    assert_eq!(plain["ok"], true, "{plain}");
}

/// 口が使う transaction は読み取り専用: 書き込みは SQLSTATE 25006 で落ち、読み取りは通る
#[tokio::test]
async fn rls_check_transaction_is_read_only() {
    let ctx = setup().await;
    let mut tx = rust_alc_api::routes::rls_check::begin_read_only(&ctx.app)
        .await
        .unwrap();

    let read_only: String = sqlx::query_scalar("SHOW transaction_read_only")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(read_only, "on");

    let err = sqlx::query("INSERT INTO alc_api.tenants (name, slug) VALUES ($1, $2)")
        .bind("RT Read Only")
        .bind(unique("read-only"))
        .execute(&mut *tx)
        .await
        .expect_err("読み取り専用の transaction で INSERT が通った");
    let code = err
        .as_database_error()
        .and_then(|e| e.code())
        .map(|c| c.to_string());
    assert_eq!(code.as_deref(), Some("25006"), "{err}");
    tx.rollback().await.unwrap();
}

/// 陽性対照: 実行用ロールに BYPASSRLS が付くと、口は ok = false を返し、検査 4 の違反を挙げる。
///
/// 属性の変更は DB 全体に効くので `ROLE_ATTRS` を write で取り、同じ binary のほかのテストを待たせる
/// (CI は shard ごとに DB が別)。ALTER と戻しの間では assert しない (途中で落ちて属性が残らないように、
/// 応答は Result のまま持ち、戻してから見る)。
#[tokio::test]
async fn rls_check_reports_bypassrls_on_runtime_role() {
    let _exclusive = ROLE_ATTRS.write().await;
    let ctx = setup_unlocked().await;
    let token = common::create_test_internal_jwt();

    let altered = sqlx::query("ALTER ROLE alc_api_rt BYPASSRLS")
        .execute(&ctx.admin)
        .await;
    let response = ctx.rls_check("", Some(&token)).await;
    sqlx::query("ALTER ROLE alc_api_rt NOBYPASSRLS")
        .execute(&ctx.admin)
        .await
        .expect("alc_api_rt を NOBYPASSRLS に戻せなかった");

    altered.expect("ALTER ROLE alc_api_rt BYPASSRLS failed");
    let (status, text) = response.unwrap();
    assert_eq!(status, 200, "{text}");
    let body: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(body["ok"], false, "{body}");
    assert_eq!(body["verdicts"]["invariants"], false, "{body}");
    assert_eq!(body["verdicts"]["runtime_role"], false, "{body}");
    assert_eq!(body["verdicts"]["migrations"], true, "{body}");
    assert_eq!(body["runtime_role"]["rolbypassrls"], true);
    assert_eq!(body["runtime_role"]["is_runtime_role"], true);
    let violations = body["invariants"]["violations"].as_array().unwrap();
    assert!(
        violations
            .iter()
            .any(|v| v["check_no"] == 4 && v["object"] == "role alc_api_rt"),
        "検査 4 の違反が無い: {body}"
    );
    assert_eq!(
        body["invariants"]["violation_count"].as_u64(),
        Some(violations.len() as u64)
    );
    for violation in violations {
        assert_eq!(sorted_keys(violation), ["check_no", "detail", "object"]);
    }
    let checks = body["invariants"]["checks"].as_array().unwrap();
    let check_4 = checks.iter().find(|c| c["check_no"] == 4).unwrap();
    assert!(check_4["violations"].as_i64().unwrap() >= 1, "{body}");
    let total: i64 = checks
        .iter()
        .map(|c| c["violations"].as_i64().unwrap())
        .sum();
    assert_eq!(total, violations.len() as i64);
    assert!(
        body["state"].is_object(),
        "違反が在っても state は返る: {body}"
    );

    // 戻した後は合格に戻る
    let after = ctx.rls_check_ok("").await;
    assert_eq!(after["runtime_role"]["rolbypassrls"], false);
    assert_eq!(after["ok"], true, "{after}");
}

/// 陽性対照: 表の状態が migration の履歴からずれると (ここでは FORCE ROW LEVEL SECURITY を付ける)、
/// 口は `drift` にその表を挙げ、`verdicts.drift` と `ok` を false にする。不変条件の合否は変わらない。
///
/// rls-check の合否は DB 全体の状態で決まるので `ROLE_ATTRS` を write で取り、同じ binary のほかの
/// テストを待たせる。ALTER と戻しの間では assert しない (途中で落ちて FORCE が残らないように、
/// 応答は Result のまま持ち、戻してから見る)。
#[tokio::test]
async fn rls_check_reports_drift_when_a_table_is_forced() {
    let _exclusive = ROLE_ATTRS.write().await;
    let ctx = setup_unlocked().await;
    let token = common::create_test_internal_jwt();

    // RLS 有効・FORCE 無しの表を 1 つ (名前の昇順の先頭)。名前はカタログから取る
    let table: String = sqlx::query_scalar(
        "SELECT c.relname::text FROM pg_class c \
          WHERE c.relnamespace = 'alc_api'::regnamespace AND c.relkind = 'r' \
            AND c.relrowsecurity AND NOT c.relforcerowsecurity \
          ORDER BY c.relname LIMIT 1",
    )
    .fetch_one(&ctx.admin)
    .await
    .unwrap();

    let altered = sqlx::query(&format!(
        "ALTER TABLE alc_api.\"{table}\" FORCE ROW LEVEL SECURITY"
    ))
    .execute(&ctx.admin)
    .await;
    let response = ctx.rls_check("", Some(&token)).await;
    sqlx::query(&format!(
        "ALTER TABLE alc_api.\"{table}\" NO FORCE ROW LEVEL SECURITY"
    ))
    .execute(&ctx.admin)
    .await
    .expect("表を NO FORCE ROW LEVEL SECURITY に戻せなかった");

    altered.expect("ALTER TABLE ... FORCE ROW LEVEL SECURITY failed");
    let (status, text) = response.unwrap();
    assert_eq!(status, 200, "{text}");
    let body: Value = serde_json::from_str(&text).unwrap();
    let drift = &body["drift"];
    assert_eq!(drift["matches_expected"], false, "{drift}");
    let tables = drift["tables"].as_array().unwrap();
    assert_eq!(tables.len(), 1, "FORCE を付けた表だけが載る: {drift}");
    let drifted = &tables[0];
    assert_eq!(drifted["name"], table.as_str());
    assert_eq!(drifted["expected"]["rls_forced"], false, "{drifted}");
    assert_eq!(drifted["actual"]["rls_forced"], true, "{drifted}");
    assert_eq!(
        sorted_keys(drifted),
        [
            "actual",
            "actual_policy_names",
            "expected",
            "expected_policy_names",
            "name"
        ]
    );
    // 所有者のロール名は、比べる対象にも、食い違いの表示にも入らない
    assert!(drifted["actual"].get("owner").is_none(), "{drifted}");
    assert_eq!(
        drifted["expected_policy_names"], drifted["actual_policy_names"],
        "{drifted}"
    );
    assert_eq!(drift["functions"], json!([]), "{drift}");
    assert!(drift["views"].is_null(), "{drift}");
    assert!(drift["sequences"].is_null(), "{drift}");
    assert_eq!(drift["unreadable"], json!([]), "{drift}");

    // 合否: drift だけが false。不変条件の検査は FORCE の有無では変わらない
    assert_eq!(
        body["verdicts"],
        json!({ "invariants": true, "runtime_role": true, "migrations": true, "drift": false }),
        "{body}"
    );
    assert_eq!(body["invariants"]["violation_count"], 0, "{body}");
    assert_eq!(body["ok"], false, "{body}");

    // 戻した後は合格に戻る
    let after = ctx.rls_check_ok("").await;
    assert_eq!(after["drift"]["matches_expected"], true, "{after}");
    assert_eq!(after["verdicts"]["drift"], true, "{after}");
    assert_eq!(after["ok"], true, "{after}");
}

// ============================================================
// 9. 運行 CSV のアップロード履歴を id で引く (Refs ippoan/auth-worker#605)
// ============================================================

/// アップロード履歴の行を 1 つ作る (RLS を素通りする側の pool で)
async fn insert_upload_history(admin: &sqlx::PgPool, tenant: Uuid, zip_key: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO alc_api.dtako_upload_history (tenant_id, filename, r2_zip_key, status) \
         VALUES ($1, 'rt.zip', $2, 'completed') RETURNING id",
    )
    .bind(tenant)
    .bind(zip_key)
    .fetch_one(admin)
    .await
    .expect("dtako_upload_history insert failed")
}

async fn upload_status(admin: &sqlx::PgPool, upload_id: Uuid) -> (String, Option<String>) {
    sqlx::query_as("SELECT status, error_message FROM alc_api.dtako_upload_history WHERE id = $1")
        .bind(upload_id)
        .fetch_one(admin)
        .await
        .unwrap()
}

fn sqlstate(err: &sqlx::Error) -> Option<String> {
    err.as_database_error()
        .and_then(|e| e.code())
        .map(|c| c.to_string())
}

/// 自分のテナントの履歴は、実行用ロールの接続で id から引けて、失敗の印も付く
#[tokio::test]
async fn upload_history_lookups_work_for_own_tenant() {
    let ctx = setup().await;
    let tenant = common::create_test_tenant(&ctx.admin, "RT Upload Own").await;
    let zip_key = format!("{tenant}/uploads/{}/rt.zip", unique("own"));
    let upload_id = insert_upload_history(&ctx.admin, tenant, &zip_key).await;
    let repo = PgDtakoUploadRepository::new(ctx.app.clone());

    let record = repo
        .get_upload_history(tenant, upload_id)
        .await
        .expect("get_upload_history failed")
        .expect("自分のテナントの履歴が見つからない");
    assert_eq!(record.tenant_id, tenant);
    assert_eq!(record.r2_zip_key, zip_key);
    assert_eq!(record.filename, "rt.zip");

    let key = repo
        .get_upload_zip_key(tenant, upload_id)
        .await
        .expect("get_upload_zip_key failed");
    assert_eq!(key.as_deref(), Some(zip_key.as_str()));

    repo.mark_upload_failed(tenant, upload_id, "boom")
        .await
        .expect("mark_upload_failed failed");
    assert_eq!(
        upload_status(&ctx.admin, upload_id).await,
        ("failed".to_string(), Some("boom".to_string()))
    );

    // 存在しない id は None
    let missing = Uuid::new_v4();
    assert!(repo
        .get_upload_history(tenant, missing)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        repo.get_upload_zip_key(tenant, missing).await.unwrap(),
        None
    );
}

/// 別テナントの `upload_id` は「見つからない」。失敗の印も付かない (行が変わらない)。
/// 口 (download / rerun) も、存在しない id と同じ 404 を返す。
#[tokio::test]
async fn upload_history_of_other_tenant_is_not_found() {
    let ctx = setup().await;
    let tenant_a = common::create_test_tenant(&ctx.admin, "RT Upload A").await;
    let tenant_b = common::create_test_tenant(&ctx.admin, "RT Upload B").await;
    let zip_key = format!("{tenant_a}/uploads/{}/rt.zip", unique("other"));
    let upload_id = insert_upload_history(&ctx.admin, tenant_a, &zip_key).await;
    let repo = PgDtakoUploadRepository::new(ctx.app.clone());

    assert!(repo
        .get_upload_history(tenant_b, upload_id)
        .await
        .expect("get_upload_history failed")
        .is_none());
    assert_eq!(
        repo.get_upload_zip_key(tenant_b, upload_id)
            .await
            .expect("get_upload_zip_key failed"),
        None
    );
    repo.mark_upload_failed(tenant_b, upload_id, "boom")
        .await
        .expect("mark_upload_failed failed");
    assert_eq!(
        upload_status(&ctx.admin, upload_id).await,
        ("completed".to_string(), None),
        "別テナントからの失敗の印で行が変わった"
    );

    let auth_b = format!("Bearer {}", common::create_test_jwt(tenant_b, "admin"));
    let res = ctx
        .client
        .get(ctx.url(&format!("/api/internal/download/{upload_id}")))
        .header("Authorization", &auth_b)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
    assert_eq!(
        res.text().await.unwrap(),
        format!("upload {upload_id} not found")
    );
    let res = ctx
        .client
        .post(ctx.url(&format!("/api/internal/rerun/{upload_id}")))
        .header("Authorization", &auth_b)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
    // split-csv の「見つからない」は、存在しない id と同じ 500
    let res = ctx
        .client
        .post(ctx.url(&format!("/api/split-csv/{upload_id}")))
        .header("Authorization", &auth_b)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
    assert_eq!(
        upload_status(&ctx.admin, upload_id).await,
        ("completed".to_string(), None)
    );

    // 持ち主のテナントからは引ける (上の「見つからない」が id の間違いでないこと)
    assert!(repo
        .get_upload_history(tenant_a, upload_id)
        .await
        .unwrap()
        .is_some());
}

/// テナントを設定しない接続 (実行用ロール) で `dtako_upload_history` を読むとエラーになる。
/// 接続が新品のとき (設定が未定義 = 42704) と、一度設定して RESET した後 (空文字 = 22P02) の両方。
/// (= id だけで引く query は、テナントを設定した接続でないと動かない)
#[tokio::test]
async fn upload_history_read_without_tenant_fails() {
    let ctx = setup().await;
    let tenant = common::create_test_tenant(&ctx.admin, "RT Upload No Tenant").await;
    let zip_key = format!("{tenant}/uploads/{}/rt.zip", unique("no-tenant"));
    let upload_id = insert_upload_history(&ctx.admin, tenant, &zip_key).await;
    const READ: &str = "SELECT count(*) FROM alc_api.dtako_upload_history WHERE id = $1";

    // 新品の接続 (pool を通さない。一度もテナントを設定していない)
    let mut conn = <sqlx::PgConnection as sqlx::Connection>::connect(&common::test_database_url())
        .await
        .expect("Failed to connect to test DB");
    sqlx::query("SET ROLE alc_api_rt")
        .execute(&mut conn)
        .await
        .unwrap();
    let err = sqlx::query_scalar::<_, i64>(READ)
        .bind(upload_id)
        .fetch_one(&mut conn)
        .await
        .expect_err("テナント未設定 (新品の接続) で dtako_upload_history を読めた");
    assert_eq!(sqlstate(&err).as_deref(), Some("42704"), "{err}");

    // 同じ接続で、テナントを設定すれば読める
    alc_core::tenant::set_current_tenant(&mut conn, &tenant.to_string())
        .await
        .unwrap();
    let visible: i64 = sqlx::query_scalar(READ)
        .bind(upload_id)
        .fetch_one(&mut conn)
        .await
        .unwrap();
    assert_eq!(visible, 1);

    // RESET した後 (pool へ返した接続と同じ状態) は、また読めない
    alc_core::tenant::reset_tenant_context(&mut conn)
        .await
        .unwrap();
    let err = sqlx::query_scalar::<_, i64>(READ)
        .bind(upload_id)
        .fetch_one(&mut conn)
        .await
        .expect_err("テナント未設定 (RESET 後の接続) で dtako_upload_history を読めた");
    assert_eq!(sqlstate(&err).as_deref(), Some("22P02"), "{err}");
}

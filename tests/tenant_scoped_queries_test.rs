//! tenant を渡して叩く query が、別テナントの id では 0 行 / 影響なしになること
//! (Refs ippoan/alc-app#387)。
//!
//! backend が表の所有者で繋いでいる間、`FORCE ROW LEVEL SECURITY` の無い表には
//! RLS が掛からず、テナントの分離は query の `WHERE tenant_id` だけが担う。
//! テスト DB の接続も RLS を素通りするので、ここで落ちる = query 側の条件が
//! 抜けている、ということ (実行用ロールへ切り替えた後は RLS が二重に止める)。
//!
//! mock (`tests/mock_helpers/`) は SQL を 1 文字も通らないので、実 DB でしか縛れない。

mod common;

use chrono::Utc;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use rust_alc_api::db::repository::{
    AuthRepository, PgAuthRepository, PgTenkoCallRepository, TenkoCallRepository,
};

async fn setup() -> sqlx::PgPool {
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::test_database_url())
        .await
        .expect("Failed to connect to test DB");
    common::migrate_and_grant(&pool).await;
    pool
}

fn unique(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::new_v4().simple())
}

/// 招待の消費 (`delete_invitation`) とログアウト (`clear_refresh_token`) は、
/// 渡したテナントの行にしか効かない。LINE の通知先の自動登録は tenant 付きで入る。
#[tokio::test]
async fn auth_repo_writes_are_tenant_scoped() {
    let pool = setup().await;
    let auth = PgAuthRepository::new(pool.clone());
    let tenant_a = common::create_test_tenant(&pool, "Scoped Auth A").await;
    let tenant_b = common::create_test_tenant(&pool, "Scoped Auth B").await;

    // --- delete_invitation ---
    let email = format!("{}@example.test", unique("invite"));
    // 招待の作成の口は alc-auth-worker へ移った (W2c) ので、行は直接入れる
    let invitation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO alc_api.tenant_allowed_emails (tenant_id, email, role) VALUES ($1, $2, 'viewer') RETURNING id",
    )
    .bind(tenant_a)
    .bind(&email)
    .fetch_one(&pool)
    .await
    .expect("invitation insert failed");

    auth.delete_invitation(tenant_b, invitation_id)
        .await
        .expect("delete_invitation (other tenant) failed");
    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM alc_api.tenant_allowed_emails WHERE id = $1")
            .bind(invitation_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(remaining, 1, "別テナントからは招待を消せないこと");

    auth.delete_invitation(tenant_a, invitation_id)
        .await
        .expect("delete_invitation failed");
    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM alc_api.tenant_allowed_emails WHERE id = $1")
            .bind(invitation_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(remaining, 0, "自テナントの招待は消えること");

    // --- clear_refresh_token ---
    let user = auth
        .create_user_line(tenant_a, &unique("line"), "Scoped User")
        .await
        .expect("create_user_line failed");
    auth.save_refresh_token(
        user.id,
        "scoped-hash",
        Utc::now() + chrono::Duration::hours(1),
    )
    .await
    .expect("save_refresh_token failed");

    auth.clear_refresh_token(tenant_b, user.id)
        .await
        .expect("clear_refresh_token (other tenant) failed");
    let hash: Option<String> =
        sqlx::query_scalar("SELECT refresh_token_hash FROM alc_api.users WHERE id = $1")
            .bind(user.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        hash.as_deref(),
        Some("scoped-hash"),
        "別テナントからは refresh token を消せないこと"
    );

    auth.clear_refresh_token(tenant_a, user.id)
        .await
        .expect("clear_refresh_token failed");
    let hash: Option<String> =
        sqlx::query_scalar("SELECT refresh_token_hash FROM alc_api.users WHERE id = $1")
            .bind(user.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(hash.is_none(), "自テナントの refresh token は消えること");

    // --- register_line_recipient ---
    let line_user_id = unique("recipient");
    auth.register_line_recipient(tenant_a, "Scoped Recipient", &line_user_id)
        .await
        .expect("register_line_recipient failed");
    let recipient_tenant: Uuid = sqlx::query_scalar(
        "SELECT tenant_id FROM alc_api.notify_recipients WHERE line_user_id = $1",
    )
    .bind(&line_user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(recipient_tenant, tenant_a);
}

/// 点呼用電話番号の削除は、渡したテナントの行だけ (予定超過の確認は W7c で alc-tenko-worker へ移った)。
#[tokio::test]
async fn tenko_repos_are_tenant_scoped() {
    let pool = setup().await;
    let tenko_call = PgTenkoCallRepository::new(pool.clone());
    let tenant_a = common::create_test_tenant(&pool, "Scoped Tenko A").await;
    let tenant_b = common::create_test_tenant(&pool, "Scoped Tenko B").await;

    // --- delete_number ---
    let call_number = unique("call");
    let number_id = tenko_call
        .create_number(&call_number, &tenant_a.to_string(), None)
        .await
        .expect("create_number failed");

    tenko_call
        .delete_number(&tenant_b.to_string(), number_id)
        .await
        .expect("delete_number (other tenant) failed");
    let left: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM alc_api.tenko_call_numbers WHERE id = $1")
            .bind(number_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(left, 1, "別テナントからは電話番号を消せないこと");

    tenko_call
        .delete_number(&tenant_a.to_string(), number_id)
        .await
        .expect("delete_number failed");
    let left: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM alc_api.tenko_call_numbers WHERE id = $1")
            .bind(number_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(left, 0);
}

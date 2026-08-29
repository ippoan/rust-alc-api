//! ログイン経路ごとの「新規ユーザーに付く role」を実 DB で固定する (Refs #599)。
//!
//! `role` は SQL 文字列リテラル / bind で決まる実行時クエリなので、コンパイル時
//! 検査が効かない。しかも `users.role` の DB 既定値は `'admin'`
//! (`migrations/003_create_users.sql`) なので、**リテラルを書き換えても列を
//! 落としても、退行は「静かに管理者が増える」形でしか現れない**。
//!
//! mock (`tests/mock_helpers/repos_a.rs`) は `create_user_lineworks` を Rust 側で
//! 組み立てて返すため、この SQL を 1 文字も通らない = ここは実 DB でしか縛れない。

mod common;

use rust_alc_api::db::repository::{AuthRepository, PgAuthRepository};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

async fn setup() -> (sqlx::PgPool, PgAuthRepository) {
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::test_database_url())
        .await
        .expect("Failed to connect to test DB");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");
    let repo = PgAuthRepository::new(pool.clone());
    (pool, repo)
}

/// 本題 (Refs #599): LINE WORKS の新規ユーザーは `viewer` で作られる。
///
/// リテラルを `'admin'` に戻すとこのテストが落ちる (陰性対照は PR 本文に実測)。
#[tokio::test]
async fn lineworks_new_user_gets_viewer_role() {
    let (pool, repo) = setup().await;
    let tenant = common::create_test_tenant(&pool, "LW Role").await;
    let lineworks_id = format!("lw-{}", Uuid::new_v4().simple());

    let user = repo
        .create_user_lineworks(
            tenant,
            &lineworks_id,
            "lw-role@example.test",
            "LW Role User",
        )
        .await
        .expect("create_user_lineworks failed");

    assert_eq!(
        user.role, "viewer",
        "LINE WORKS の新規ユーザーは viewer であること (SQL リテラル / 列の抜け落ちで admin に戻る)"
    );
    assert_eq!(user.lineworks_id.as_deref(), Some(lineworks_id.as_str()));
    assert_eq!(user.tenant_id, tenant);

    // DB に書かれた値そのものも見る (RETURNING * の取り違えではないこと)
    let stored: String = sqlx::query_scalar("SELECT role FROM users WHERE id = $1")
        .bind(user.id)
        .fetch_one(&pool)
        .await
        .expect("select role failed");
    assert_eq!(stored, "viewer");
}

/// 既存ユーザーの role は #599 の変更では動かない。
///
/// `upsert_lineworks_user` (`crates/alc-auth/src/internal.rs`) は
/// `find_user_by_lineworks_id` がヒットしたらその行をそのまま返し、
/// `create_user_lineworks` を呼ばない。その分岐条件そのものを固定する。
#[tokio::test]
async fn existing_lineworks_user_is_found_and_role_untouched() {
    let (pool, repo) = setup().await;
    let tenant = common::create_test_tenant(&pool, "LW Existing").await;
    let lineworks_id = format!("lw-{}", Uuid::new_v4().simple());

    let created = repo
        .create_user_lineworks(tenant, &lineworks_id, "lw-exist@example.test", "Existing")
        .await
        .expect("create_user_lineworks failed");

    // 既に admin に昇格済みの人を模す
    sqlx::query("UPDATE users SET role = 'admin' WHERE id = $1")
        .bind(created.id)
        .execute(&pool)
        .await
        .expect("promote failed");

    let found = repo
        .find_user_by_lineworks_id(&lineworks_id)
        .await
        .expect("find_user_by_lineworks_id failed")
        .expect("既存ユーザーが引けること");

    assert_eq!(found.id, created.id);
    assert_eq!(
        found.role, "admin",
        "既存行がヒットする限り role は据え置き (create 側の変更は効かない)"
    );
}

/// 触っていない 2 経路の role が動いていないこと。
/// LINE = `viewer` リテラル / Google = 招待の role を bind (可変)。
#[tokio::test]
async fn line_stays_viewer_and_google_still_binds_role() {
    let (pool, repo) = setup().await;
    let tenant = common::create_test_tenant(&pool, "Other Routes Role").await;

    let line_user = repo
        .create_user_line(
            tenant,
            &format!("line-{}", Uuid::new_v4().simple()),
            "LINE User",
        )
        .await
        .expect("create_user_line failed");
    assert_eq!(line_user.role, "viewer");

    for role in ["admin", "viewer"] {
        let google_user = repo
            .create_user_google(
                tenant,
                &format!("gsub-{}", Uuid::new_v4().simple()),
                "google-role@example.test",
                "Google User",
                role,
            )
            .await
            .expect("create_user_google failed");
        assert_eq!(
            google_user.role, role,
            "Google 経路は招待の role をそのまま bind する"
        );
    }
}

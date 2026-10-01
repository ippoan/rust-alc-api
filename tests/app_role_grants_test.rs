//! テスト DB の `alc_api_app` の表の権限が本番と揃っているかの検査 (Refs #685)。
//!
//! 本番 (Supabase) は `alc_api` の全表に `alc_api_app` の SELECT がある。初期の表の
//! GRANT は migration の外で付いたので、テスト DB では migration の後に
//! `scripts/local_app_grants.sql` で再現している (`common::migrate_and_grant`)。
//!
//! 新しい表を作る migration で GRANT を付け忘れると、本番の `alc_api_app` からは
//! 読めない (過去に 502 の実害。migration 151 のコメント)。ここで SELECT の無い表が
//! 1 つでもあれば落とす。直し方は migration に `GRANT ... TO alc_api_app` を書くこと
//! (`local_app_grants.sql` へ足すのは本番に既にある GRANT だけ)。

mod common;

use sqlx::postgres::PgPoolOptions;

#[tokio::test]
async fn every_alc_api_table_is_selectable_by_alc_api_app() {
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::test_database_url())
        .await
        .expect("Failed to connect to test DB");
    common::migrate_and_grant(&pool).await;

    let tables: Vec<(String,)> = sqlx::query_as(
        "SELECT c.relname::text FROM pg_class c \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = 'alc_api' AND c.relkind IN ('r', 'p') \
         ORDER BY c.relname",
    )
    .fetch_all(&pool)
    .await
    .expect("Failed to list alc_api tables");
    assert!(
        tables.len() > 50,
        "alc_api の表が {} 件しか無い (migration が流れていない?)",
        tables.len()
    );

    let missing: Vec<(String,)> = sqlx::query_as(
        "SELECT c.relname::text FROM pg_class c \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = 'alc_api' AND c.relkind IN ('r', 'p') \
           AND NOT has_table_privilege('alc_api_app', c.oid, 'SELECT') \
         ORDER BY c.relname",
    )
    .fetch_all(&pool)
    .await
    .expect("Failed to check alc_api_app privileges");
    let missing: Vec<String> = missing.into_iter().map(|(t,)| t).collect();

    assert!(
        missing.is_empty(),
        "alc_api_app に SELECT が無い alc_api の表が {} 件: {}\n\
         新しい表なら、その表を作る migration に GRANT ... TO alc_api_app を書く",
        missing.len(),
        missing.join(", ")
    );
}

/// backend の実行用ロール `alc_api_rt` (alc-migrations 158) の権限。
///
/// こちらは `scripts/local_app_grants.sql` に写しを持たない — 権限は migration 158 の GRANT と
/// `ALTER DEFAULT PRIVILEGES` だけで付く。写しを足すと 158 の付け漏れをここで検出できなくなる。
/// 直すのは migration 側 (新しい表は DEFAULT PRIVILEGES で付く。PUBLIC から REVOKE した関数は
/// `alc_api_rt` へ明示の GRANT が要る)。
#[tokio::test]
async fn alc_api_rt_has_runtime_privileges_and_nothing_on_sqlx_migrations() {
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&common::test_database_url())
        .await
        .expect("Failed to connect to test DB");
    common::migrate_and_grant(&pool).await;

    // ロールの立場: superuser でも BYPASSRLS でもなく、表を 1 つも所有していない (= RLS が掛かる)
    let (is_super, bypass_rls): (bool, bool) =
        sqlx::query_as("SELECT rolsuper, rolbypassrls FROM pg_roles WHERE rolname = 'alc_api_rt'")
            .fetch_one(&pool)
            .await
            .expect("ロール alc_api_rt が無い (migration 158 が流れていない?)");
    assert!(!is_super && !bypass_rls);
    let owned: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_class c \
         WHERE c.relnamespace = 'alc_api'::regnamespace \
           AND pg_get_userbyid(c.relowner) = 'alc_api_rt'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(owned, 0, "alc_api_rt が alc_api の表を所有している");

    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT has_schema_privilege('alc_api_rt', 'alc_api', 'USAGE')"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        "alc_api_rt に schema alc_api の USAGE が無い"
    );

    // 表: _sqlx_migrations 以外の全表に SELECT / INSERT / UPDATE / DELETE
    let missing: Vec<(String,)> = sqlx::query_as(
        "SELECT format('%s: %s', c.relname, p.priv) FROM pg_class c \
         CROSS JOIN (VALUES ('SELECT'), ('INSERT'), ('UPDATE'), ('DELETE')) AS p(priv) \
         WHERE c.relnamespace = 'alc_api'::regnamespace AND c.relkind IN ('r', 'p') \
           AND c.relname <> '_sqlx_migrations' \
           AND NOT has_table_privilege('alc_api_rt', c.oid, p.priv) \
         ORDER BY 1",
    )
    .fetch_all(&pool)
    .await
    .expect("Failed to check alc_api_rt table privileges");
    let missing: Vec<String> = missing.into_iter().map(|(t,)| t).collect();
    assert!(
        missing.is_empty(),
        "alc_api_rt に付いていない表の権限が {} 件: {}",
        missing.len(),
        missing.join(", ")
    );

    // _sqlx_migrations は backend が読まない。権限が無いのが正
    let on_migrations: Vec<(String,)> = sqlx::query_as(
        "SELECT p.priv FROM pg_class c \
         CROSS JOIN (VALUES ('SELECT'), ('INSERT'), ('UPDATE'), ('DELETE')) AS p(priv) \
         WHERE c.relnamespace = 'alc_api'::regnamespace AND c.relname = '_sqlx_migrations' \
           AND has_table_privilege('alc_api_rt', c.oid, p.priv)",
    )
    .fetch_all(&pool)
    .await
    .expect("Failed to check _sqlx_migrations privileges");
    assert!(
        on_migrations.is_empty(),
        "alc_api_rt が _sqlx_migrations に権限を持っている: {on_migrations:?}"
    );

    // SECURITY DEFINER 関数: 全部呼べる (認証前・テナント横断の query はこれ経由)
    let not_executable: Vec<(String,)> = sqlx::query_as(
        "SELECT f.oid::regprocedure::text FROM pg_proc f \
         WHERE f.pronamespace = 'alc_api'::regnamespace AND f.prosecdef \
           AND NOT has_function_privilege('alc_api_rt', f.oid, 'EXECUTE') \
         ORDER BY 1",
    )
    .fetch_all(&pool)
    .await
    .expect("Failed to check alc_api_rt function privileges");
    let not_executable: Vec<String> = not_executable.into_iter().map(|(f,)| f).collect();
    assert!(
        not_executable.is_empty(),
        "alc_api_rt が呼べない SECURITY DEFINER 関数が {} 件: {}",
        not_executable.len(),
        not_executable.join(", ")
    );
}

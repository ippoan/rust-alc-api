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

//! dtako 取り込みの乗務員解決 (`code` 優先) と、既存の二重登録を畳む migration 149 を
//! 実 DB で固定する (Refs ippoan/rust-alc-api#669)。
//!
//! employees への書き込み経路は 2 つあり、キーが噛み合っていなかった:
//!
//! | 経路 | キー | 入れる列 |
//! |---|---|---|
//! | theearth 乗務員マスタ同期 (`upsert_by_code`) | `code` | `code` (正本) |
//! | dtako 運行取り込み (`upsert_driver`) | `driver_cd` | `driver_cd` |
//!
//! 同じ乗務員CD が両方の列に入っていても紐付かないため、管理画面に「社員番号あり」と
//! 「社員番号 `-`」の 2 行が並ぶ。
//!
//! mock リポジトリ (tests/mock_helpers/repos_b.rs) は**引数を無視する固定値スタブ**なので
//! この解決順は mock では検証できない。SQL も `sqlx::query_as` の実行時クエリで
//! コンパイル時検査が効かないため、実 DB で縛る。

mod common;

use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use alc_core::repository::dtako_upload::DtakoUploadRepository;
use rust_alc_api::db::repository::PgDtakoUploadRepository;

/// migration 149 の本体。テスト DB には起動時に既に適用済みなので、**同じファイルを
/// もう一度流して**仕込んだ重複に対する振る舞いを見る (DO ブロックは「今ある重複を
/// 畳む」ので何度流しても同じ意味になる)。
const DEDUP_MIGRATION: &str = include_str!("../migrations/149_employees_dedup_by_driver_cd.sql");

async fn setup_pool() -> sqlx::PgPool {
    let url = common::test_database_url();
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("Failed to connect to test DB");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");
    pool
}

/// employees を 1 行仕込む。`code` / `driver_cd` / `deleted_at` の組み合わせがこのテストの本題。
/// `created_at` は勝者決定 (`ORDER BY (code IS NOT NULL) DESC, created_at, id`) に効くので明示する。
async fn insert_employee(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    code: Option<&str>,
    driver_cd: Option<&str>,
    name: &str,
    created_at: &str,
    deleted: bool,
) -> Uuid {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO alc_api.employees \
             (tenant_id, nfc_id, name, code, driver_cd, created_at, deleted_at) \
         VALUES ($1, $2, $3, $4, $5, $6::TIMESTAMPTZ, \
                 CASE WHEN $7 THEN NOW() ELSE NULL END) \
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(Uuid::new_v4().to_string())
    .bind(name)
    .bind(code)
    .bind(driver_cd)
    .bind(created_at)
    .bind(deleted)
    .fetch_one(pool)
    .await
    .expect("Failed to insert employee");
    row.0
}

async fn driver_cd_of(pool: &sqlx::PgPool, id: Uuid) -> Option<String> {
    sqlx::query_scalar("SELECT driver_cd FROM alc_api.employees WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("Failed to read driver_cd")
}

async fn live_employee_count(pool: &sqlx::PgPool, tenant_id: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*)::BIGINT FROM alc_api.employees \
         WHERE tenant_id = $1 AND deleted_at IS NULL",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .expect("Failed to count employees")
}

// ---------------------------------------------------------------------------
// B. upsert_driver / get_employee_id_by_driver_cd の解決ラダー
// ---------------------------------------------------------------------------

#[tokio::test]
async fn upsert_driver_reuses_code_row_and_backfills_driver_cd() {
    let pool = setup_pool().await;
    let tenant_id = common::create_test_tenant(&pool, "dedup code row").await;
    // theearth 同期が入れた正本 (code だけ・driver_cd は NULL)
    let canonical = insert_employee(
        &pool,
        tenant_id,
        Some("7001"),
        None,
        "正本 7001",
        "2026-01-01T00:00:00Z",
        false,
    )
    .await;
    let repo = PgDtakoUploadRepository::new(pool.clone());

    let got = repo
        .upsert_driver(tenant_id, "7001", "デジタコ 7001")
        .await
        .expect("upsert_driver failed");

    assert_eq!(got, Some(canonical), "正本を再利用する (新規行を作らない)");
    assert_eq!(
        driver_cd_of(&pool, canonical).await,
        Some("7001".to_string()),
        "以後 driver_cd 検索で直接引けるようバックフィルする"
    );
    assert_eq!(
        live_employee_count(&pool, tenant_id).await,
        1,
        "行は増えない"
    );
}

#[tokio::test]
async fn upsert_driver_falls_back_to_driver_cd_row() {
    let pool = setup_pool().await;
    let tenant_id = common::create_test_tenant(&pool, "dedup driver_cd row").await;
    let existing = insert_employee(
        &pool,
        tenant_id,
        None,
        Some("7002"),
        "dtako 7002",
        "2026-01-01T00:00:00Z",
        false,
    )
    .await;
    let repo = PgDtakoUploadRepository::new(pool.clone());

    let got = repo
        .upsert_driver(tenant_id, "7002", "デジタコ 7002")
        .await
        .expect("upsert_driver failed");

    assert_eq!(
        got,
        Some(existing),
        "code 一致が無ければ従来どおり driver_cd で引く"
    );
    assert_eq!(live_employee_count(&pool, tenant_id).await, 1);
}

#[tokio::test]
async fn upsert_driver_inserts_when_neither_key_matches() {
    let pool = setup_pool().await;
    let tenant_id = common::create_test_tenant(&pool, "dedup new row").await;
    let repo = PgDtakoUploadRepository::new(pool.clone());

    let got = repo
        .upsert_driver(tenant_id, "7003", "デジタコ 7003")
        .await
        .expect("upsert_driver failed")
        .expect("new employee expected");

    assert_eq!(live_employee_count(&pool, tenant_id).await, 1);
    let code: Option<String> =
        sqlx::query_scalar("SELECT code FROM alc_api.employees WHERE id = $1")
            .bind(got)
            .fetch_one(&pool)
            .await
            .expect("Failed to read code");
    assert_eq!(
        code, None,
        "dtako 側は社員番号を知らないので code は入れない"
    );
    assert_eq!(driver_cd_of(&pool, got).await, Some("7003".to_string()));
}

#[tokio::test]
async fn upsert_driver_skips_backfill_when_another_live_row_holds_driver_cd() {
    let pool = setup_pool().await;
    let tenant_id = common::create_test_tenant(&pool, "dedup backfill conflict").await;
    // 正本 (code だけ) と、既に driver_cd を持つ dtako 由来の行が同居している状態。
    // ここでバックフィルすると同じ driver_cd の生存行が 2 つになるので埋めない。
    let canonical = insert_employee(
        &pool,
        tenant_id,
        Some("7004"),
        None,
        "正本 7004",
        "2026-01-01T00:00:00Z",
        false,
    )
    .await;
    let dtako_row = insert_employee(
        &pool,
        tenant_id,
        None,
        Some("7004"),
        "dtako 7004",
        "2026-02-01T00:00:00Z",
        false,
    )
    .await;
    let repo = PgDtakoUploadRepository::new(pool.clone());

    let got = repo
        .upsert_driver(tenant_id, "7004", "デジタコ 7004")
        .await
        .expect("upsert_driver failed");

    assert_eq!(
        got,
        Some(dtako_row),
        "バックフィルせず driver_cd 経路へ落ちる"
    );
    assert_eq!(
        driver_cd_of(&pool, canonical).await,
        None,
        "重複を増やさないため正本は NULL のまま"
    );
    assert_eq!(
        live_employee_count(&pool, tenant_id).await,
        2,
        "行は増えない"
    );
}

#[tokio::test]
async fn upsert_driver_ignores_soft_deleted_rows() {
    let pool = setup_pool().await;
    let tenant_id = common::create_test_tenant(&pool, "dedup soft deleted").await;
    insert_employee(
        &pool,
        tenant_id,
        Some("7005"),
        None,
        "退職 7005 (code)",
        "2026-01-01T00:00:00Z",
        true,
    )
    .await;
    insert_employee(
        &pool,
        tenant_id,
        None,
        Some("7005"),
        "退職 7005 (driver_cd)",
        "2026-01-02T00:00:00Z",
        true,
    )
    .await;
    let repo = PgDtakoUploadRepository::new(pool.clone());

    let got = repo
        .upsert_driver(tenant_id, "7005", "デジタコ 7005")
        .await
        .expect("upsert_driver failed")
        .expect("new employee expected");

    assert_eq!(
        live_employee_count(&pool, tenant_id).await,
        1,
        "soft-delete 済みは解決対象にならず、新規行が 1 つできる"
    );
    assert_eq!(driver_cd_of(&pool, got).await, Some("7005".to_string()));
}

#[tokio::test]
async fn get_employee_id_by_driver_cd_resolves_by_code_without_writing() {
    let pool = setup_pool().await;
    let tenant_id = common::create_test_tenant(&pool, "dedup lookup by code").await;
    let canonical = insert_employee(
        &pool,
        tenant_id,
        Some("7006"),
        None,
        "正本 7006",
        "2026-01-01T00:00:00Z",
        false,
    )
    .await;
    let repo = PgDtakoUploadRepository::new(pool.clone());

    let got = repo
        .get_employee_id_by_driver_cd(tenant_id, "7006")
        .await
        .expect("get_employee_id_by_driver_cd failed");

    assert_eq!(got, Some(canonical), "code 一致でも解決できる");
    assert_eq!(
        driver_cd_of(&pool, canonical).await,
        None,
        "読み取り関数なので書き込まない (バックフィルは upsert_driver だけ)"
    );
}

// ---------------------------------------------------------------------------
// D. migration 149 の振る舞い
// ---------------------------------------------------------------------------

async fn insert_operation(pool: &sqlx::PgPool, tenant_id: Uuid, driver_id: Uuid, unko_no: &str) {
    sqlx::query(
        "INSERT INTO alc_api.dtako_operations \
             (tenant_id, unko_no, crew_role, reading_date, driver_id, raw_data) \
         VALUES ($1, $2, 1, '2026-06-19', $3, '{}'::JSONB)",
    )
    .bind(tenant_id)
    .bind(unko_no)
    .bind(driver_id)
    .execute(pool)
    .await
    .expect("Failed to insert operation");
}

async fn insert_daily_work_hours(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    driver_id: Uuid,
    work_date: &str,
) {
    sqlx::query(
        "INSERT INTO alc_api.dtako_daily_work_hours \
             (tenant_id, driver_id, work_date, start_time) \
         VALUES ($1, $2, $3::DATE, '08:00:00')",
    )
    .bind(tenant_id)
    .bind(driver_id)
    .bind(work_date)
    .execute(pool)
    .await
    .expect("Failed to insert daily work hours");
}

async fn insert_work_segment(pool: &sqlx::PgPool, tenant_id: Uuid, driver_id: Uuid, unko_no: &str) {
    sqlx::query(
        "INSERT INTO alc_api.dtako_daily_work_segments \
             (tenant_id, driver_id, work_date, unko_no, segment_index, start_at, end_at, work_minutes) \
         VALUES ($1, $2, '2026-06-19', $3, 0, '2026-06-19T08:00:00Z', '2026-06-19T17:00:00Z', 540)",
    )
    .bind(tenant_id)
    .bind(driver_id)
    .bind(unko_no)
    .execute(pool)
    .await
    .expect("Failed to insert work segment");
}

async fn run_dedup_migration(pool: &sqlx::PgPool) {
    sqlx::raw_sql(DEDUP_MIGRATION)
        .execute(pool)
        .await
        .expect("dedup migration failed");
}

async fn deleted_at_is_set(pool: &sqlx::PgPool, id: Uuid) -> bool {
    let row: Option<(bool,)> =
        sqlx::query_as("SELECT deleted_at IS NOT NULL FROM alc_api.employees WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await
            .expect("Failed to read deleted_at");
    row.expect("行が物理削除されている (soft-delete のはず)").0
}

async fn operation_driver(pool: &sqlx::PgPool, unko_no: &str) -> Option<Uuid> {
    sqlx::query_scalar("SELECT driver_id FROM alc_api.dtako_operations WHERE unko_no = $1")
        .bind(unko_no)
        .fetch_one(pool)
        .await
        .expect("Failed to read operation driver_id")
}

#[tokio::test]
async fn migration_collapses_code_and_driver_cd_pair_and_moves_dtako_rows() {
    let pool = setup_pool().await;
    let tenant_id = common::create_test_tenant(&pool, "dedup migration pair").await;
    // 正本 (code のみ) と dtako 由来の重複 (driver_cd のみ)。実際の症状そのもの。
    let canonical = insert_employee(
        &pool,
        tenant_id,
        Some("7101"),
        None,
        "正本 7101",
        "2026-01-01T00:00:00Z",
        false,
    )
    .await;
    let duplicate = insert_employee(
        &pool,
        tenant_id,
        None,
        Some("7101"),
        "dtako 7101",
        "2026-02-01T00:00:00Z",
        false,
    )
    .await;
    let unko_no = format!("A{}", Uuid::new_v4().simple());
    insert_operation(&pool, tenant_id, duplicate, &unko_no).await;
    insert_daily_work_hours(&pool, tenant_id, duplicate, "2026-06-19").await;
    insert_work_segment(&pool, tenant_id, duplicate, &unko_no).await;

    run_dedup_migration(&pool).await;

    assert_eq!(
        live_employee_count(&pool, tenant_id).await,
        1,
        "1 行に畳まれる"
    );
    assert_eq!(
        driver_cd_of(&pool, canonical).await,
        Some("7101".to_string()),
        "正本に driver_cd がバックフィルされる"
    );
    assert!(
        deleted_at_is_set(&pool, duplicate).await,
        "敗者は soft-delete"
    );
    assert_eq!(
        operation_driver(&pool, &unko_no).await,
        Some(canonical),
        "dtako_operations.driver_id が勝者を指す"
    );
    let hours_driver: Uuid = sqlx::query_scalar(
        "SELECT driver_id FROM alc_api.dtako_daily_work_hours \
         WHERE tenant_id = $1 AND work_date = '2026-06-19'",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to read daily work hours driver_id");
    assert_eq!(
        hours_driver, canonical,
        "dtako_daily_work_hours も勝者を指す"
    );
    let segment_driver: Uuid = sqlx::query_scalar(
        "SELECT driver_id FROM alc_api.dtako_daily_work_segments WHERE unko_no = $1",
    )
    .bind(&unko_no)
    .fetch_one(&pool)
    .await
    .expect("Failed to read segment driver_id");
    assert_eq!(
        segment_driver, canonical,
        "dtako_daily_work_segments も勝者を指す"
    );
}

#[tokio::test]
async fn migration_collapses_group_without_code_row() {
    let pool = setup_pool().await;
    let tenant_id = common::create_test_tenant(&pool, "dedup migration no code").await;
    // code を持つ行が 1 つも無い重複 (取り込みの競合などで dtako 側だけが 2 行できた形)。
    // 「code 側に対がある重複」に限定すると畳めないので、ここも対象にする。
    let older = insert_employee(
        &pool,
        tenant_id,
        None,
        Some("7102"),
        "dtako 7102 (古)",
        "2026-01-01T00:00:00Z",
        false,
    )
    .await;
    let newer = insert_employee(
        &pool,
        tenant_id,
        None,
        Some("7102"),
        "dtako 7102 (新)",
        "2026-02-01T00:00:00Z",
        false,
    )
    .await;
    let unko_no = format!("B{}", Uuid::new_v4().simple());
    insert_operation(&pool, tenant_id, newer, &unko_no).await;

    run_dedup_migration(&pool).await;

    assert_eq!(live_employee_count(&pool, tenant_id).await, 1);
    assert!(
        !deleted_at_is_set(&pool, older).await,
        "created_at が古い方が勝者"
    );
    assert!(deleted_at_is_set(&pool, newer).await);
    assert_eq!(operation_driver(&pool, &unko_no).await, Some(older));
}

#[tokio::test]
async fn migration_soft_deletes_losers_without_physical_delete() {
    let pool = setup_pool().await;
    let tenant_id = common::create_test_tenant(&pool, "dedup migration soft delete").await;
    let canonical = insert_employee(
        &pool,
        tenant_id,
        Some("7103"),
        None,
        "正本 7103",
        "2026-01-01T00:00:00Z",
        false,
    )
    .await;
    let duplicate = insert_employee(
        &pool,
        tenant_id,
        None,
        Some("7103"),
        "dtako 7103",
        "2026-02-01T00:00:00Z",
        false,
    )
    .await;

    run_dedup_migration(&pool).await;

    // 物理削除していれば fetch_optional が None になり deleted_at_is_set が panic する
    assert!(
        deleted_at_is_set(&pool, duplicate).await,
        "敗者は残っている"
    );
    assert!(!deleted_at_is_set(&pool, canonical).await, "勝者は生存");
    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::BIGINT FROM alc_api.employees WHERE tenant_id = $1")
            .bind(tenant_id)
            .fetch_one(&pool)
            .await
            .expect("Failed to count employees");
    assert_eq!(total, 2, "行数は減らない (deleted_at を入れるだけ)");
}

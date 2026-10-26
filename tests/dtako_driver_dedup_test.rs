//! dtako 取り込みの乗務員解決 (`code` 優先、Refs ippoan/rust-alc-api#669)、復活時の
//! `driver_cd` 衝突ガード、および migration 150 の部分一意 index
//! (Refs ippoan/rust-alc-api#673) を実 DB で固定する。
//! dtako 取り込み側 (`upsert_driver` / `get_employee_id_by_driver_cd`) の解決は
//! ippoan/alc-dtako-worker が持ち、そちらの実 DB テストが縛る (Refs ippoan/rust-alc-api#725)。
//!
//! employees への書き込み経路は 2 つあり、キーが噛み合っていなかった:
//!
//! | 経路 | キー | 入れる列 |
//! |---|---|---|
//! | theearth 乗務員マスタ同期 (`upsert_by_code`、alc-people-worker へ移った) | `code` | `code` (正本) |
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

/// migration 150 が張る部分一意 index の名前
/// (`UNIQUE (tenant_id, driver_cd) WHERE driver_cd IS NOT NULL AND deleted_at IS NULL`)。
const LIVE_DRIVER_CD_INDEX: &str = "idx_employees_tenant_driver_cd_live";

async fn setup_pool() -> sqlx::PgPool {
    let url = common::test_database_url();
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("Failed to connect to test DB");
    common::migrate_and_grant(&pool).await;
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
    try_insert_employee(pool, tenant_id, code, driver_cd, name, created_at, deleted)
        .await
        .expect("Failed to insert employee")
}

/// `insert_employee` の Result 版。migration 150 の一意 index に当てて Err を見るテストが使う。
async fn try_insert_employee(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    code: Option<&str>,
    driver_cd: Option<&str>,
    name: &str,
    created_at: &str,
    deleted: bool,
) -> Result<Uuid, sqlx::Error> {
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
    .await?;
    Ok(row.0)
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
// D. migration 149 の振る舞い — **テストは置かない** (Refs ippoan/rust-alc-api#673)
// ---------------------------------------------------------------------------
//
// ここには 149 (`149_employees_dedup_by_driver_cd.sql`) を実 DB で流し直して畳み方を
// 見るテストが 3 本あったが、migration 150 で部分一意 index を張ったので削除した。
//
//   * **149 は適用済みで、sqlx の checksum (SHA-384) に凍結されている。** 中身を書き換える
//     ことが機械的に不可能なので、「149 のロジックが壊れる」回帰は発生しない
//   * **150 の index が在ると 149 のシナリオは構造的に再現できない。** 149 の step 1 は
//     敗者 (同じ driver_cd の生存行) を soft-delete する**前に**勝者へ driver_cd を
//     バックフィルするので、その UPDATE が必ず index に違反する。149 の冒頭コメントが
//     「一意 index は張らない … 復活経路を塞ぐのとセットで別 PR にする」と書いた制約そのもの
//   * **まっさらな DB では 149 は空の `employees` に対して走る**ので無害
//     (重複が 0 件なら step 1 の `EXISTS` が 1 行も一致せず、違反しない)
//
// ⇒ 残せる回帰が無く、維持するにはテストの中から本番の制約を DROP するしかないので置かない。
// 再発防止として今後効いているのは F 節 (index そのもの) と E 節 (復活時のガード、いまは alc-people-worker 側)。

// ---------------------------------------------------------------------------
// E. upsert_by_code の復活と driver_cd の衝突 — ippoan/alc-people-worker へ移った
// ---------------------------------------------------------------------------
//
// 社員マスタ同期 (`PUT /api/employees/bulk-by-code`) は W4d-b で alc-people-worker へ移り、
// 復活時の driver_cd / nfc_id の衝突はそちらの実 DB テスト (`crates/people/tests/employees_db.rs`) が縛る。

// ---------------------------------------------------------------------------
// F. migration 150 の部分一意 index (Refs ippoan/rust-alc-api#673)
// ---------------------------------------------------------------------------
//
// 149 は既存の重複を畳んだだけで再発は止めていなかった。違反しうる書き手 (復活 2 経路と
// dtako の新規 INSERT) を #674 で塞いだので、150 で
// `UNIQUE (tenant_id, driver_cd) WHERE driver_cd IS NOT NULL AND deleted_at IS NULL`
// を張った。ここで縛るのは**述語が効いていること**:
//   * 生存 2 行が同じ (tenant_id, driver_cd) を持てない (再発防止そのもの)
//   * 片方が soft-delete 済みなら通る (`deleted_at IS NULL` が無いと退職者の行が
//     driver_cd を握ったままになる)
//   * driver_cd 未設定の行は何行でも並ぶ (theearth 同期だけで作られた行を弾かない)

#[tokio::test]
async fn unique_index_rejects_second_live_row_with_same_driver_cd() {
    let pool = setup_pool().await;
    let tenant_id = common::create_test_tenant(&pool, "unique driver_cd live").await;
    insert_employee(
        &pool,
        tenant_id,
        None,
        Some("7301"),
        "dtako 7301",
        "2026-01-01T00:00:00Z",
        false,
    )
    .await;

    let err = try_insert_employee(
        &pool,
        tenant_id,
        None,
        Some("7301"),
        "二重登録 7301",
        "2026-02-01T00:00:00Z",
        false,
    )
    .await
    .expect_err("生存 2 行が同じ (tenant_id, driver_cd) を持てない");

    let db_err = err
        .as_database_error()
        .expect("一意違反は database error として返る");
    assert_eq!(
        db_err.constraint(),
        Some(LIVE_DRIVER_CD_INDEX),
        "落とすのは 150 の index (code や nfc_id の一意制約ではない)"
    );
    assert_eq!(
        live_employee_count(&pool, tenant_id).await,
        1,
        "2 行目は入らない"
    );
}

#[tokio::test]
async fn unique_index_allows_same_driver_cd_when_the_other_row_is_soft_deleted() {
    let pool = setup_pool().await;
    let tenant_id = common::create_test_tenant(&pool, "unique driver_cd deleted").await;
    // 退職して soft-delete された行が driver_cd を握っている状態。
    insert_employee(
        &pool,
        tenant_id,
        None,
        Some("7302"),
        "退職 7302",
        "2026-01-01T00:00:00Z",
        true,
    )
    .await;

    let live = insert_employee(
        &pool,
        tenant_id,
        None,
        Some("7302"),
        "dtako 7302",
        "2026-02-01T00:00:00Z",
        false,
    )
    .await;

    assert_eq!(
        driver_cd_of(&pool, live).await,
        Some("7302".to_string()),
        "deleted_at IS NULL の述語が効いているので、削除済みの行とは衝突しない"
    );
    assert_eq!(live_employee_count(&pool, tenant_id).await, 1);
}

#[tokio::test]
async fn unique_index_allows_many_live_rows_without_driver_cd() {
    let pool = setup_pool().await;
    let tenant_id = common::create_test_tenant(&pool, "unique driver_cd null").await;
    // theearth 同期だけで作られた行は driver_cd が NULL のまま並ぶ。
    for (i, code) in ["7401", "7402", "7403"].iter().enumerate() {
        insert_employee(
            &pool,
            tenant_id,
            Some(code),
            None,
            &format!("正本 {code}"),
            &format!("2026-01-0{}T00:00:00Z", i + 1),
            false,
        )
        .await;
    }

    assert_eq!(
        live_employee_count(&pool, tenant_id).await,
        3,
        "driver_cd IS NOT NULL の述語が効いているので、未設定の行同士は衝突しない"
    );
}

//! dtako 取り込みの乗務員解決 (`code` 優先、Refs ippoan/rust-alc-api#669)、復活時の
//! `driver_cd` 衝突ガード、および migration 150 の部分一意 index
//! (Refs ippoan/rust-alc-api#673) を実 DB で固定する。
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

use alc_core::models::EmployeeUpsertItem;
use alc_core::repository::dtako_upload::DtakoUploadRepository;
use alc_core::repository::employees::EmployeeRepository;
use rust_alc_api::db::repository::{PgDtakoUploadRepository, PgEmployeeRepository};

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
// 再発防止として今後効いているのは F 節 (index そのもの) と E 節 (復活時のガード)。

async fn deleted_at_is_set(pool: &sqlx::PgPool, id: Uuid) -> bool {
    let row: Option<(bool,)> =
        sqlx::query_as("SELECT deleted_at IS NOT NULL FROM alc_api.employees WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await
            .expect("Failed to read deleted_at");
    row.expect("行が物理削除されている (soft-delete のはず)").0
}

// ---------------------------------------------------------------------------
// E. upsert_by_code の復活と driver_cd の衝突 (Refs ippoan/rust-alc-api#673)
// ---------------------------------------------------------------------------
//
// `upsert_by_code` の code 検索は **deleted_at を問わない** (idx_employees_code が
// deleted_at を見ない一意制約なので、削除済み行を無視すると INSERT が衝突する)。
// そのため論理削除済みの行も `deleted_at = NULL` で復活する。復活行が driver_cd を
// 持ち、同じ値の生存行が別に居ると、後続 PR で張る
// `UNIQUE (tenant_id, driver_cd) WHERE driver_cd IS NOT NULL AND deleted_at IS NULL`
// に違反して **1 人のせいでバッチ全体が 500** になる。
//
// ここで縛るのは **driver_cd が NULL に落ちること**と **skipped の中身**。
// index そのもの (述語が効いているか) は下の F 節が縛る。

/// 既知の nfc_id を後から入れる (insert_employee は衝突回避のため毎回ランダムを入れる)。
async fn set_nfc_id(pool: &sqlx::PgPool, id: Uuid, nfc_id: &str) {
    sqlx::query("UPDATE alc_api.employees SET nfc_id = $2 WHERE id = $1")
        .bind(id)
        .bind(nfc_id)
        .execute(pool)
        .await
        .expect("Failed to set nfc_id");
}

async fn nfc_id_of(pool: &sqlx::PgPool, id: Uuid) -> Option<String> {
    sqlx::query_scalar("SELECT nfc_id FROM alc_api.employees WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("Failed to read nfc_id")
}

fn upsert_item(code: &str, name: &str, nfc_id: Option<&str>) -> EmployeeUpsertItem {
    EmployeeUpsertItem {
        code: code.to_string(),
        name: name.to_string(),
        nfc_id: nfc_id.map(|s| s.to_string()),
        license_issue_date: None,
        license_expiry_date: None,
    }
}

#[tokio::test]
async fn upsert_by_code_drops_driver_cd_when_reviving_into_a_conflict() {
    let pool = setup_pool().await;
    let tenant_id = common::create_test_tenant(&pool, "revive driver_cd conflict").await;
    // 復活対象 (code 一致・driver_cd 持ち・論理削除済み)
    let revived = insert_employee(
        &pool,
        tenant_id,
        Some("7201"),
        Some("7201"),
        "退職 7201",
        "2026-01-01T00:00:00Z",
        true,
    )
    .await;
    // 同じ driver_cd を持つ生存行 (dtako 取り込みが作った側)
    let live = insert_employee(
        &pool,
        tenant_id,
        None,
        Some("7201"),
        "dtako 7201",
        "2026-02-01T00:00:00Z",
        false,
    )
    .await;
    let repo = PgEmployeeRepository::new(pool.clone());

    let summary = repo
        .upsert_by_code(tenant_id, &[upsert_item("7201", "復職 7201", None)])
        .await
        .expect("トランザクションごと落ちない (1 人の衝突で全員分を 500 にしない)");

    assert_eq!(summary.updated, 1, "復活も更新もさせる (continue しない)");
    assert!(
        !deleted_at_is_set(&pool, revived).await,
        "deleted_at = NULL で復活する"
    );
    assert_eq!(
        driver_cd_of(&pool, revived).await,
        None,
        "衝突しているので復活行の driver_cd は落とす"
    );
    assert_eq!(
        driver_cd_of(&pool, live).await,
        Some("7201".to_string()),
        "生存行の driver_cd は触らない"
    );
    let reasons: Vec<&str> = summary.skipped.iter().map(|s| s.reason.as_str()).collect();
    assert_eq!(reasons, vec!["driver_cd_conflict"]);
    assert_eq!(summary.skipped[0].code, "7201");
}

#[tokio::test]
async fn upsert_by_code_keeps_driver_cd_when_no_conflict() {
    let pool = setup_pool().await;
    let tenant_id = common::create_test_tenant(&pool, "revive driver_cd kept").await;
    let revived = insert_employee(
        &pool,
        tenant_id,
        Some("7202"),
        Some("7202"),
        "退職 7202",
        "2026-01-01T00:00:00Z",
        true,
    )
    .await;
    let repo = PgEmployeeRepository::new(pool.clone());

    let summary = repo
        .upsert_by_code(tenant_id, &[upsert_item("7202", "復職 7202", None)])
        .await
        .expect("upsert_by_code failed");

    assert_eq!(summary.updated, 1);
    assert!(!deleted_at_is_set(&pool, revived).await, "復活する");
    assert_eq!(
        driver_cd_of(&pool, revived).await,
        Some("7202".to_string()),
        "衝突相手が居なければ driver_cd は据え置き"
    );
    assert!(
        summary.skipped.is_empty(),
        "skipped は空 (衝突していないので理由が無い)"
    );
}

#[tokio::test]
async fn upsert_by_code_reports_nfc_id_and_driver_cd_conflicts_separately() {
    let pool = setup_pool().await;
    let tenant_id = common::create_test_tenant(&pool, "revive both conflicts").await;
    let revived = insert_employee(
        &pool,
        tenant_id,
        Some("7203"),
        Some("7203"),
        "退職 7203",
        "2026-01-01T00:00:00Z",
        true,
    )
    .await;
    let revived_nfc = nfc_id_of(&pool, revived).await;
    // nfc_id も driver_cd も抱えている生存行。両方の衝突が同時に立つ。
    let live = insert_employee(
        &pool,
        tenant_id,
        None,
        Some("7203"),
        "dtako 7203",
        "2026-02-01T00:00:00Z",
        false,
    )
    .await;
    set_nfc_id(&pool, live, "20260101202612310000").await;
    let repo = PgEmployeeRepository::new(pool.clone());

    let summary = repo
        .upsert_by_code(
            tenant_id,
            &[upsert_item(
                "7203",
                "復職 7203",
                Some("20260101202612310000"),
            )],
        )
        .await
        .expect("upsert_by_code failed");

    assert_eq!(summary.updated, 1);
    assert!(!deleted_at_is_set(&pool, revived).await, "復活する");
    assert_eq!(
        nfc_id_of(&pool, revived).await,
        revived_nfc,
        "nfc_id は据え置き (衝突している側は触らない)"
    );
    assert_eq!(
        driver_cd_of(&pool, revived).await,
        None,
        "nfc_id 衝突の分岐でも driver_cd は落とす"
    );
    // ★ 理由ごとに 1 行。運用側が原因を読み分けられるよう、同じ code が 2 エントリ並ぶ。
    let reasons: Vec<&str> = summary.skipped.iter().map(|s| s.reason.as_str()).collect();
    assert_eq!(reasons, vec!["nfc_id_conflict", "driver_cd_conflict"]);
    assert!(summary.skipped.iter().all(|s| s.code == "7203"));
}

#[tokio::test]
async fn upsert_driver_does_not_error_when_insert_hits_a_unique_index() {
    let pool = setup_pool().await;
    let tenant_id = common::create_test_tenant(&pool, "dtako insert on conflict").await;
    // 論理削除済みの行が driver_cd を握っている。生存行を見る 2 つの SELECT は
    // これを拾わないので、解決は INSERT まで進む。
    insert_employee(
        &pool,
        tenant_id,
        None,
        Some("7204"),
        "退職 7204",
        "2026-01-01T00:00:00Z",
        true,
    )
    .await;
    // INSERT を確実に衝突させるための **この tenant 限定の代用 index**
    // (後続 PR が張る本物の index ではない。本物は述語に deleted_at IS NULL を持つ)。
    // 他のテストと同じ DB を共有するので、tenant_id で閉じて巻き込まないようにする。
    let index_name = format!("idx_test_driver_cd_{}", Uuid::new_v4().simple());
    sqlx::query(&format!(
        "CREATE UNIQUE INDEX {index_name} ON alc_api.employees (tenant_id, driver_cd) \
         WHERE driver_cd IS NOT NULL AND tenant_id = '{tenant_id}'::UUID"
    ))
    .execute(&pool)
    .await
    .expect("Failed to create test index");

    let repo = PgDtakoUploadRepository::new(pool.clone());
    let got = repo.upsert_driver(tenant_id, "7204", "デジタコ 7204").await;

    sqlx::query(&format!("DROP INDEX alc_api.{index_name}"))
        .execute(&pool)
        .await
        .expect("Failed to drop test index");

    // 素の INSERT なら unique 違反で Err になり、取り込み 1 件が 500 になっていた。
    let got = got.expect("ON CONFLICT DO NOTHING なので Err にならない");
    assert_eq!(
        got, None,
        "衝突して 1 行も入らず、引き直しても生存行が無いので乗務員解決は None"
    );
    assert_eq!(
        live_employee_count(&pool, tenant_id).await,
        0,
        "行は増えない"
    );
}

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

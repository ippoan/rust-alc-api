//! `DtakoYTimeExportRepository` の期間条件を実 DB で固定する
//! (Refs ohishi-exp/rust-ichibanboshi#205 の 38)。
//!
//! この 4 メソッドの `WHERE` は
//! `(reading_date BETWEEN from-1 AND to+1 OR operation_date BETWEEN from-1 AND to+1)`。
//! **読取日だけで絞ると月末に走った運行が構造的に落ちる** — 読取日はカードを読ませた日
//! なので、月末の運行ほど読まれるのが翌月上旬になる。2026-06 の勤怠が オンプレ基準より
//! 142 行少なかった原因がこれで、名指しできた 29 件は 29/29 が「alc に存在し・読取日が
//! 窓の外 (07-03〜07-13)・運行日は窓の中 (06-24〜07-01)」だった。
//!
//! SQL は `sqlx::query_as` の実行時クエリでコンパイル時検査が効かない。窓が縮む方向の
//! 退行は「静かにデータが減る」形で出るため、実 DB で 4 メソッドとも縛る。
//!
//! 4 本の運行を仕込んで境界を固定する (乗務員 1078 の実データを模した B が本題):
//!
//! | | 読取日 | 運行日 | 期待 |
//! |---|---|---|---|
//! | A | 窓の中 | 窓の中 | 拾う (対照) |
//! | B | **窓の外** (07-06) | **窓の中** (06-24) | **拾う** ← この PR で拾えるようになる |
//! | C | 窓の外 | 窓の外 | 拾わない (窓が消えていないことの確認) |
//! | D | 窓の中 | **NULL** | 拾う (OR にしても NULL 行が落ちない) |

mod common;

use chrono::NaiveDate;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use alc_core::repository::dtako_y_time_export::DtakoYTimeExportRepository;
use rust_alc_api::db::repository::PgDtakoYTimeExportRepository;

/// 呼び出し側 (ichibanboshi の月ゲート) が 2026-06 に対して渡す窓。
/// repo 側が ±1 日広げるので実効は `2026-05-31 ..= 2026-07-02`。
const FROM: (i32, u32, u32) = (2026, 6, 1);
const TO: (i32, u32, u32) = (2026, 7, 1);

fn d(t: (i32, u32, u32)) -> NaiveDate {
    NaiveDate::from_ymd_opt(t.0, t.1, t.2).unwrap()
}

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

async fn create_driver(pool: &sqlx::PgPool, tenant_id: Uuid, driver_cd: &str) -> Uuid {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO alc_api.employees (tenant_id, nfc_id, name, driver_cd) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(tenant_id)
    .bind(Uuid::new_v4().to_string())
    .bind(format!("運転手 {driver_cd}"))
    .bind(driver_cd)
    .fetch_one(pool)
    .await
    .expect("Failed to create driver");
    row.0
}

/// 1 運行を仕込む。`operation_date` が `None` の行は「KUDGURI に運行日が無い取り込み」。
async fn insert_operation(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    driver_id: Uuid,
    unko_no: &str,
    reading_date: NaiveDate,
    operation_date: Option<NaiveDate>,
    has_kudgivt: bool,
) {
    sqlx::query(
        "INSERT INTO alc_api.dtako_operations \
             (tenant_id, unko_no, crew_role, reading_date, operation_date, driver_id, \
              has_kudgivt, raw_data) \
         VALUES ($1, $2, 1, $3, $4, $5, $6, '{}'::JSONB)",
    )
    .bind(tenant_id)
    .bind(unko_no)
    .bind(reading_date)
    .bind(operation_date)
    .bind(driver_id)
    .bind(has_kudgivt)
    .execute(pool)
    .await
    .expect("Failed to insert operation");
}

/// A/B/C/D の 4 本を仕込み、`(tenant_id, driver_id)` を返す。
/// `has_kudgivt` は `list_unsplit_operations` 以外の 3 メソッドが TRUE で絞るので引数にする。
async fn seed(pool: &sqlx::PgPool, label: &str, has_kudgivt: bool) -> (Uuid, Uuid) {
    let tenant_id = common::create_test_tenant(pool, label).await;
    let driver_id = create_driver(pool, tenant_id, "1078").await;
    let p = Uuid::new_v4().simple().to_string();
    // A: 読取日も運行日も窓の中 (乗務員 1021 `2606190748...` = 欠落ゼロだった対照)
    let a = format!("A{p}");
    insert_operation(
        pool,
        tenant_id,
        driver_id,
        &a,
        d((2026, 6, 19)),
        Some(d((2026, 6, 19))),
        has_kudgivt,
    )
    .await;
    // B: 読取日 07-06 (窓の外) / 運行日 06-24 (窓の中)。乗務員 1078 `2606241140...` の形
    let b = format!("B{p}");
    insert_operation(
        pool,
        tenant_id,
        driver_id,
        &b,
        d((2026, 7, 6)),
        Some(d((2026, 6, 24))),
        has_kudgivt,
    )
    .await;
    // C: どちらも窓の外 (翌月に走って翌月に読まれた運行)
    let c = format!("C{p}");
    insert_operation(
        pool,
        tenant_id,
        driver_id,
        &c,
        d((2026, 7, 20)),
        Some(d((2026, 7, 20))),
        has_kudgivt,
    )
    .await;
    // D: 運行日が NULL で読取日だけ窓の中。reading_date 側の条件を消すと落ちる行
    let dd = format!("D{p}");
    insert_operation(
        pool,
        tenant_id,
        driver_id,
        &dd,
        d((2026, 6, 10)),
        None,
        has_kudgivt,
    )
    .await;
    (tenant_id, driver_id)
}

/// `unko_no` の先頭 1 文字 (A/B/C/D) を並べて返す。UUID 部分は seed ごとに違うので落とす。
fn labels(mut unko_nos: Vec<String>) -> Vec<String> {
    unko_nos.sort();
    unko_nos
        .into_iter()
        .map(|u| u.chars().take(1).collect())
        .collect()
}

#[tokio::test]
async fn list_operations_takes_reading_date_or_operation_date() {
    let pool = setup_pool().await;
    let (tenant_id, driver_id) = seed(&pool, "op window single", true).await;
    let repo = PgDtakoYTimeExportRepository::new(pool);

    let ops = repo
        .list_operations(tenant_id, driver_id, d(FROM), d(TO))
        .await
        .expect("list_operations failed");

    let got = labels(ops.into_iter().map(|o| o.unko_no).collect());
    assert_eq!(got, vec!["A", "B", "D"], "B (運行日だけ窓の中) が要る");
}

#[tokio::test]
async fn list_operations_for_drivers_takes_reading_date_or_operation_date() {
    let pool = setup_pool().await;
    let (tenant_id, driver_id) = seed(&pool, "op window multi", true).await;
    let repo = PgDtakoYTimeExportRepository::new(pool);

    let ops = repo
        .list_operations_for_drivers(tenant_id, &[driver_id], d(FROM), d(TO))
        .await
        .expect("list_operations_for_drivers failed");

    let got = labels(ops.into_iter().map(|o| o.unko_no).collect());
    assert_eq!(got, vec!["A", "B", "D"], "全乗務員版も同じ条件で拾う");
}

#[tokio::test]
async fn list_unsplit_operations_takes_reading_date_or_operation_date() {
    let pool = setup_pool().await;
    // has_kudgivt = FALSE 側。他 3 メソッドと窓が同じであることを固定する
    let (tenant_id, _driver_id) = seed(&pool, "op window unsplit", false).await;
    let repo = PgDtakoYTimeExportRepository::new(pool);

    let rows = repo
        .list_unsplit_operations(tenant_id, d(FROM), d(TO))
        .await
        .expect("list_unsplit_operations failed");

    let got = labels(rows.into_iter().map(|r| r.unko_no).collect());
    assert_eq!(got, vec!["A", "B", "D"], "未 split 側も同じ窓で数える");
}

#[tokio::test]
async fn list_drivers_with_operations_finds_a_driver_whose_only_operation_is_read_late() {
    let pool = setup_pool().await;
    // B (読取日だけ窓の外) しか持たない乗務員。読取日だけで絞ると 1 件も返らない
    let tenant_id = common::create_test_tenant(&pool, "op window drivers").await;
    let driver_id = create_driver(&pool, tenant_id, "1078").await;
    insert_operation(
        &pool,
        tenant_id,
        driver_id,
        &format!("B{}", Uuid::new_v4().simple()),
        d((2026, 7, 6)),
        Some(d((2026, 6, 24))),
        true,
    )
    .await;
    let repo = PgDtakoYTimeExportRepository::new(pool);

    let drivers = repo
        .list_drivers_with_operations(tenant_id, d(FROM), d(TO), None, 100)
        .await
        .expect("list_drivers_with_operations failed");

    let cds: Vec<String> = drivers.into_iter().map(|d| d.driver_cd).collect();
    assert_eq!(
        cds,
        vec!["1078".to_string()],
        "最終運行が翌月読みでも列挙する"
    );
}

//! 運行の変更記録 (`dtako_operation_changes`、migration 152) を実 DB で固定する
//! (Refs ohishi-exp/nuxt-dtako-admin#1133)。
//!
//! 手動削除の記録は `delete_by_unko_no` の実行時 SQL (旧行の読み出し → 消す → 残す) で
//! 決まり、mock リポジトリは引数を無視する固定値スタブなので検証できない。
//! 上げ直し (reupload) の記録は ippoan/alc-dtako-worker が持ち、そちらの実 DB テストが縛る
//! (Refs ippoan/rust-alc-api#725)。ここでは前準備の行を SQL で直に入れる。
//!
//! | | 操作 | 期待 |
//! |---|---|---|
//! | (d) | 手動削除 | manual_delete・after=NULL が crew_role ごとに |
//! | (f) | 読み口 | 別 tenant の行を返さない |

mod common;

use chrono::NaiveDate;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use alc_core::repository::dtako_operations::DtakoOperationsRepository;
use rust_alc_api::db::repository::PgDtakoOperationsRepository;

const UNKO: &str = "2605230341010000004219";

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

/// 取り込み済みの運行を 1 行入れる (上げ直しの経路は worker 側にあるので SQL で直に)。
async fn insert_operation(pool: &sqlx::PgPool, tenant_id: Uuid, crew_role: i32, driver_id: Uuid) {
    let dt = |h| {
        NaiveDate::from_ymd_opt(2026, 5, 23)
            .unwrap()
            .and_hms_opt(h, 41, 1)
    };
    sqlx::query(
        "INSERT INTO alc_api.dtako_operations \
           (tenant_id, unko_no, crew_role, reading_date, operation_date, driver_id, \
            departure_at, return_at, raw_data, r2_key_prefix) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(tenant_id)
    .bind(UNKO)
    .bind(crew_role)
    .bind(NaiveDate::from_ymd_opt(2026, 5, 24).unwrap())
    .bind(NaiveDate::from_ymd_opt(2026, 5, 23))
    .bind(driver_id)
    .bind(dt(3))
    .bind(dt(22))
    .bind(serde_json::json!({}))
    .bind(format!("{tenant_id}/unko/{UNKO}"))
    .execute(pool)
    .await
    .expect("Failed to insert operation");
}

/// 上げ直しの変更記録 (休憩 0 → 178) を 1 行入れる。
async fn insert_reupload_change(pool: &sqlx::PgPool, tenant_id: Uuid, driver_cd: &str) {
    sqlx::query(
        "INSERT INTO alc_api.dtako_operation_changes \
           (tenant_id, unko_no, crew_role, driver_cd, upload_id, reason, before, after) \
         VALUES ($1, $2, 1, $3, $4, 'reupload', $5, $6)",
    )
    .bind(tenant_id)
    .bind(UNKO)
    .bind(driver_cd)
    .bind(Uuid::new_v4())
    .bind(serde_json::json!({ "driver_cd": driver_cd, "break_minutes": 0 }))
    .bind(serde_json::json!({ "driver_cd": driver_cd, "break_minutes": 178 }))
    .execute(pool)
    .await
    .expect("Failed to insert operation change");
}

/// `(crew_role, reason, driver_cd, before, after)` を crew_role 順に返す。
async fn changes(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
) -> Vec<(
    i32,
    String,
    Option<String>,
    Option<serde_json::Value>,
    Option<serde_json::Value>,
)> {
    sqlx::query_as(
        "SELECT crew_role, reason, driver_cd, before, after \
           FROM alc_api.dtako_operation_changes WHERE tenant_id = $1 \
          ORDER BY crew_role, recorded_at",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn test_manual_delete_records_each_crew_role() {
    let pool = setup_pool().await;
    let tenant_id = common::create_test_tenant(&pool, "opchg-delete").await;
    let main_driver = create_driver(&pool, tenant_id, "1738").await;
    let assistant = create_driver(&pool, tenant_id, "1194").await;
    insert_operation(&pool, tenant_id, 1, main_driver).await;
    insert_operation(&pool, tenant_id, 2, assistant).await;

    // (d) crew_role をまとめて消すと、crew_role ごとに manual_delete・after=NULL
    let ops = PgDtakoOperationsRepository::new(pool.clone());
    assert_eq!(ops.delete_by_unko_no(tenant_id, UNKO).await.unwrap(), 2);
    let rows = changes(&pool, tenant_id).await;
    assert_eq!(rows.len(), 2);
    for ((crew_role, reason, driver_cd, before, after), (want_role, want_cd)) in
        rows.iter().zip([(1, "1738"), (2, "1194")])
    {
        assert_eq!(*crew_role, want_role);
        assert_eq!(reason, "manual_delete");
        assert_eq!(driver_cd.as_deref(), Some(want_cd));
        assert_eq!(before.as_ref().unwrap()["driver_cd"], want_cd);
        assert!(after.is_none());
    }

    // 無い運行を消しても記録は増えない
    assert_eq!(ops.delete_by_unko_no(tenant_id, UNKO).await.unwrap(), 0);
    assert_eq!(changes(&pool, tenant_id).await.len(), 2);
}

#[tokio::test]
async fn test_list_does_not_return_other_tenant() {
    let pool = setup_pool().await;
    let tenant_a = common::create_test_tenant(&pool, "opchg-tenant-a").await;
    let tenant_b = common::create_test_tenant(&pool, "opchg-tenant-b").await;
    for tenant_id in [tenant_a, tenant_b] {
        let driver = create_driver(&pool, tenant_id, "1194").await;
        insert_operation(&pool, tenant_id, 1, driver).await;
        insert_reupload_change(&pool, tenant_id, "1194").await;
    }

    // (f) 同じ乗務員CD・同じ運行NO でも、別 tenant の行は返さない。テストは superuser
    // 接続 (RLS を素通り) なので、ここで縛れるのは SQL の WHERE tenant_id の方
    let ops = PgDtakoOperationsRepository::new(pool.clone());
    let a = ops.list_operation_changes(tenant_a, "1194").await.unwrap();
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].unko_no, UNKO);
    assert_eq!(a[0].reason, "reupload");
    let since_a = ops
        .operation_changes_recording_since(tenant_a)
        .await
        .unwrap();
    assert!(since_a.is_some());
    let tenant_c = common::create_test_tenant(&pool, "opchg-tenant-c").await;
    assert!(ops
        .operation_changes_recording_since(tenant_c)
        .await
        .unwrap()
        .is_none());
}

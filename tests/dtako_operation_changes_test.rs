//! 運行の変更記録 (`dtako_operation_changes`、migration 152) を実 DB で固定する
//! (Refs ohishi-exp/nuxt-dtako-admin#1133)。
//!
//! 記録の有無は `replace_operation` / `delete_by_unko_no` の実行時 SQL (旧行の読み出し
//! → 消す → 入れる → 比べる → 残す) で決まり、mock リポジトリは引数を無視する固定値
//! スタブなので検証できない。
//!
//! | | 操作 | 期待 |
//! |---|---|---|
//! | (a) | 休憩 0 → 178 で上げ直し | reupload が before/after 付きで 1 行 |
//! | (b) | 同じ中身で上げ直し | 0 行 |
//! | (c) | 初回取り込み | 0 行 |
//! | (d) | 手動削除 | manual_delete・after=NULL が crew_role ごとに |
//! | (e) | 2マンの運行を上げ直し | crew_role ごとに正しい乗務員・分数で対応 |
//! | (f) | 読み口 | 別 tenant の行を返さない |

mod common;

use chrono::NaiveDate;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use alc_core::repository::dtako_operations::DtakoOperationsRepository;
use alc_core::repository::dtako_upload::{
    DtakoUploadRepository, InsertOperationParams, OperationMinutes, ReuploadChangeInput,
};
use rust_alc_api::db::repository::{PgDtakoOperationsRepository, PgDtakoUploadRepository};

const UNKO: &str = "2605230341010000004219";

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

fn params(tenant_id: Uuid, crew_role: i32, driver_id: Uuid) -> InsertOperationParams {
    let dt = |h| {
        NaiveDate::from_ymd_opt(2026, 5, 23)
            .unwrap()
            .and_hms_opt(h, 41, 1)
    };
    InsertOperationParams {
        tenant_id,
        unko_no: UNKO.to_string(),
        crew_role,
        reading_date: NaiveDate::from_ymd_opt(2026, 5, 24).unwrap(),
        operation_date: NaiveDate::from_ymd_opt(2026, 5, 23),
        office_id: None,
        vehicle_id: None,
        driver_id: Some(driver_id),
        departure_at: dt(3),
        return_at: dt(22),
        garage_out_at: None,
        garage_in_at: None,
        meter_start: None,
        meter_end: None,
        total_distance: None,
        drive_time_general: None,
        drive_time_highway: None,
        drive_time_bypass: None,
        safety_score: None,
        economy_score: None,
        total_score: None,
        raw_data: serde_json::json!({}),
        r2_key_prefix: format!("{tenant_id}/unko/{UNKO}"),
    }
}

fn minutes(break_minutes: i32) -> OperationMinutes {
    OperationMinutes {
        drive_minutes: 300,
        cargo_minutes: 20,
        break_minutes,
        rest_minutes: 480,
    }
}

fn change(before: Option<i32>, after: i32) -> ReuploadChangeInput {
    ReuploadChangeInput {
        upload_id: Uuid::new_v4(),
        before_minutes: before.map(minutes),
        after_minutes: minutes(after),
    }
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
async fn test_first_import_records_nothing() {
    let pool = setup_pool().await;
    let tenant_id = common::create_test_tenant(&pool, "opchg-first").await;
    let driver = create_driver(&pool, tenant_id, "1194").await;
    let repo = PgDtakoUploadRepository::new(pool.clone());

    // (c) 旧行が無い初回取り込みは記録しない
    assert!(!repo.operation_exists(tenant_id, UNKO, 1).await.unwrap());
    let recorded = repo
        .replace_operation(tenant_id, &params(tenant_id, 1, driver), &change(None, 0))
        .await
        .unwrap();
    assert!(!recorded);
    assert!(repo.operation_exists(tenant_id, UNKO, 1).await.unwrap());
    assert!(changes(&pool, tenant_id).await.is_empty());
}

#[tokio::test]
async fn test_reupload_records_only_when_changed() {
    let pool = setup_pool().await;
    let tenant_id = common::create_test_tenant(&pool, "opchg-reupload").await;
    let driver = create_driver(&pool, tenant_id, "1194").await;
    let repo = PgDtakoUploadRepository::new(pool.clone());
    repo.replace_operation(tenant_id, &params(tenant_id, 1, driver), &change(None, 0))
        .await
        .unwrap();

    // (b) 同じ中身で上げ直すと記録しない
    let recorded = repo
        .replace_operation(
            tenant_id,
            &params(tenant_id, 1, driver),
            &change(Some(0), 0),
        )
        .await
        .unwrap();
    assert!(!recorded);
    assert!(changes(&pool, tenant_id).await.is_empty());

    // 旧 KUDGIVT が取れなかった回は分数を比べない (出退社と乗務員が同じなら 0 行)
    let recorded = repo
        .replace_operation(tenant_id, &params(tenant_id, 1, driver), &change(None, 178))
        .await
        .unwrap();
    assert!(!recorded);
    assert!(changes(&pool, tenant_id).await.is_empty());

    // (a) 休憩 0 → 178 で上げ直すと 1 行
    let c = change(Some(0), 178);
    let recorded = repo
        .replace_operation(tenant_id, &params(tenant_id, 1, driver), &c)
        .await
        .unwrap();
    assert!(recorded);
    let rows = changes(&pool, tenant_id).await;
    assert_eq!(rows.len(), 1);
    let (crew_role, reason, driver_cd, before, after) = &rows[0];
    assert_eq!(*crew_role, 1);
    assert_eq!(reason, "reupload");
    assert_eq!(driver_cd.as_deref(), Some("1194"));
    let before = before.as_ref().unwrap();
    let after = after.as_ref().unwrap();
    assert_eq!(before["break_minutes"], 0);
    assert_eq!(after["break_minutes"], 178);
    assert_eq!(before["driver_cd"], "1194");
    assert_eq!(before["departure_at"], "2026-05-23T03:41:01Z");
    assert_eq!(after["return_at"], "2026-05-23T22:41:01Z");

    let (upload_id,): (Option<Uuid>,) = sqlx::query_as(
        "SELECT upload_id FROM alc_api.dtako_operation_changes WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(upload_id, Some(c.upload_id));
}

#[tokio::test]
async fn test_two_man_operation_maps_crew_roles() {
    let pool = setup_pool().await;
    let tenant_id = common::create_test_tenant(&pool, "opchg-2man").await;
    let main_driver = create_driver(&pool, tenant_id, "1738").await;
    let assistant = create_driver(&pool, tenant_id, "1194").await;
    let other = create_driver(&pool, tenant_id, "1500").await;
    let repo = PgDtakoUploadRepository::new(pool.clone());
    repo.replace_operation(
        tenant_id,
        &params(tenant_id, 1, main_driver),
        &change(None, 60),
    )
    .await
    .unwrap();
    repo.replace_operation(
        tenant_id,
        &params(tenant_id, 2, assistant),
        &change(None, 0),
    )
    .await
    .unwrap();

    // (e) 主は同じ中身、助手だけ休憩 0 → 178 かつ乗務員が 1194 → 1500 に付け替わった
    assert!(!repo
        .replace_operation(
            tenant_id,
            &params(tenant_id, 1, main_driver),
            &change(Some(60), 60)
        )
        .await
        .unwrap());
    assert!(repo
        .replace_operation(
            tenant_id,
            &params(tenant_id, 2, other),
            &change(Some(0), 178)
        )
        .await
        .unwrap());

    let rows = changes(&pool, tenant_id).await;
    assert_eq!(rows.len(), 1);
    let (crew_role, _, driver_cd, before, after) = &rows[0];
    assert_eq!(*crew_role, 2);
    assert_eq!(driver_cd.as_deref(), Some("1500"));
    assert_eq!(before.as_ref().unwrap()["driver_cd"], "1194");
    assert_eq!(after.as_ref().unwrap()["driver_cd"], "1500");
    assert_eq!(before.as_ref().unwrap()["break_minutes"], 0);
    assert_eq!(after.as_ref().unwrap()["break_minutes"], 178);

    // 読み口は前後どちらの乗務員CD でも引ける
    let ops = PgDtakoOperationsRepository::new(pool.clone());
    assert_eq!(
        ops.list_operation_changes(tenant_id, "1194")
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        ops.list_operation_changes(tenant_id, "1500")
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(ops
        .list_operation_changes(tenant_id, "1738")
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn test_manual_delete_records_each_crew_role() {
    let pool = setup_pool().await;
    let tenant_id = common::create_test_tenant(&pool, "opchg-delete").await;
    let main_driver = create_driver(&pool, tenant_id, "1738").await;
    let assistant = create_driver(&pool, tenant_id, "1194").await;
    let upload = PgDtakoUploadRepository::new(pool.clone());
    upload
        .replace_operation(
            tenant_id,
            &params(tenant_id, 1, main_driver),
            &change(None, 0),
        )
        .await
        .unwrap();
    upload
        .replace_operation(
            tenant_id,
            &params(tenant_id, 2, assistant),
            &change(None, 0),
        )
        .await
        .unwrap();

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
    let upload = PgDtakoUploadRepository::new(pool.clone());
    for tenant_id in [tenant_a, tenant_b] {
        let driver = create_driver(&pool, tenant_id, "1194").await;
        upload
            .replace_operation(tenant_id, &params(tenant_id, 1, driver), &change(None, 0))
            .await
            .unwrap();
        upload
            .replace_operation(
                tenant_id,
                &params(tenant_id, 1, driver),
                &change(Some(0), 178),
            )
            .await
            .unwrap();
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

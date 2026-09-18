//! driver_info の「ト 車両整備状況」節に足した整備記録の解決
//! (`PgDriverInfoRepository::get_recent_maintenance_records`) を実 DB で固定する。
//! Refs ippoan/rust-alc-api#651
//!
//! `tenko_sessions.carins_vehicle_id` → `maintenance_vehicles.car_id` →
//! `maintenance_records` の解決は 1 CTE の生 SQL (`sqlx::query_as` の実行時クエリで
//! コンパイル時検査が効かない) なので、mock テスト (`tests/mock_tests/
//! mock_driver_info_test.rs`) では 1 行も通らない。ここで NULL / 未紐づけ / 他テナント
//! の分岐を実 DB で固定する。
//!
//! 回し方: `make db-up && source .test-config && cargo test --test driver_info_maintenance_test`
//! CI は ci.yml の DB 付き shard。

mod common;

use uuid::Uuid;

use rust_alc_api::db::repository::driver_info::DriverInfoRepository;
use rust_alc_api::db::repository::PgDriverInfoRepository;

/// テスト用従業員を DB 直 INSERT で作る (HTTP 経由の `create_test_employee` は
/// admin ログインが要るため、ここでは repository のテストに専念する)
async fn insert_employee(pool: &sqlx::PgPool, tenant_id: Uuid) -> Uuid {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO alc_api.employees (tenant_id, nfc_id, name) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind(Uuid::new_v4().to_string())
    .bind("Test Driver")
    .fetch_one(pool)
    .await
    .expect("employee insert failed");
    row.0
}

/// 点呼セッションを 1 件仕込む (`carins_vehicle_id` は None なら NULL)
async fn insert_tenko_session(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    employee_id: Uuid,
    carins_vehicle_id: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO alc_api.tenko_sessions (tenant_id, employee_id, tenko_type, carins_vehicle_id) \
         VALUES ($1, $2, 'normal', $3)",
    )
    .bind(tenant_id)
    .bind(employee_id)
    .bind(carins_vehicle_id)
    .execute(pool)
    .await
    .expect("tenko_sessions insert failed");
}

/// 整備車両マスタを 1 件仕込む (`car_id` は None なら NULL = carins 未紐づけ)
async fn insert_vehicle(pool: &sqlx::PgPool, tenant_id: Uuid, car_id: Option<&str>) -> Uuid {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO alc_api.maintenance_vehicles (tenant_id, registration_number, car_id) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(tenant_id)
    .bind("品川500あ1234")
    .bind(car_id)
    .fetch_one(pool)
    .await
    .expect("maintenance_vehicles insert failed");
    row.0
}

async fn insert_category(pool: &sqlx::PgPool, tenant_id: Uuid) -> Uuid {
    let row: (Uuid,) = sqlx::query_as(
        "INSERT INTO alc_api.maintenance_categories (tenant_id, name) VALUES ($1, $2) RETURNING id",
    )
    .bind(tenant_id)
    .bind("定期点検")
    .fetch_one(pool)
    .await
    .expect("maintenance_categories insert failed");
    row.0
}

#[allow(clippy::too_many_arguments)]
async fn insert_record(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
    vehicle_id: Uuid,
    category_id: Uuid,
    performed_on: chrono::NaiveDate,
) {
    sqlx::query(
        "INSERT INTO alc_api.maintenance_records \
         (tenant_id, vehicle_id, category_id, performed_on, vendor, cost) \
         VALUES ($1, $2, $3, $4, 'Test Motors', 5500.00)",
    )
    .bind(tenant_id)
    .bind(vehicle_id)
    .bind(category_id)
    .bind(performed_on)
    .execute(pool)
    .await
    .expect("maintenance_records insert failed");
}

/// 1. 整備記録がある車両で、直近の記録が返る
#[tokio::test]
async fn returns_recent_records_for_linked_vehicle() {
    let state = common::setup_app_state().await;
    let pool = state.pool();
    let tenant_id = common::create_test_tenant(pool, "Maintenance DriverInfo A").await;
    let employee_id = insert_employee(pool, tenant_id).await;

    insert_tenko_session(pool, tenant_id, employee_id, Some("CARID-001")).await;
    let vehicle_id = insert_vehicle(pool, tenant_id, Some("CARID-001")).await;
    let category_id = insert_category(pool, tenant_id).await;
    insert_record(
        pool,
        tenant_id,
        vehicle_id,
        category_id,
        chrono::NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
    )
    .await;

    let repo = PgDriverInfoRepository::new(pool.clone());
    let records = repo
        .get_recent_maintenance_records(tenant_id, employee_id)
        .await
        .expect("query failed");

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].vehicle_id, vehicle_id);
    assert_eq!(records[0].category_name, "定期点検");
    assert_eq!(records[0].vendor.as_deref(), Some("Test Motors"));
    assert_eq!(records[0].cost.as_deref(), Some("5500.00"));
}

/// 2. carins_vehicle_id が NULL のセッションで空配列が返る (404/500 にならない)
#[tokio::test]
async fn returns_empty_when_carins_vehicle_id_is_null() {
    let state = common::setup_app_state().await;
    let pool = state.pool();
    let tenant_id = common::create_test_tenant(pool, "Maintenance DriverInfo B").await;
    let employee_id = insert_employee(pool, tenant_id).await;

    // carins 未タップの点呼セッション (carins_vehicle_id = NULL)
    insert_tenko_session(pool, tenant_id, employee_id, None).await;
    // 車両・整備記録自体は存在する (別の CarId に紐づいている) が、
    // このセッションからは解決できないはず
    let vehicle_id = insert_vehicle(pool, tenant_id, Some("CARID-999")).await;
    let category_id = insert_category(pool, tenant_id).await;
    insert_record(
        pool,
        tenant_id,
        vehicle_id,
        category_id,
        chrono::NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
    )
    .await;

    let repo = PgDriverInfoRepository::new(pool.clone());
    let records = repo
        .get_recent_maintenance_records(tenant_id, employee_id)
        .await
        .expect("query failed");

    assert!(records.is_empty());
}

/// 3. maintenance_vehicles.car_id が NULL (carins 未紐づけ) で空配列が返る
#[tokio::test]
async fn returns_empty_when_vehicle_car_id_is_null() {
    let state = common::setup_app_state().await;
    let pool = state.pool();
    let tenant_id = common::create_test_tenant(pool, "Maintenance DriverInfo C").await;
    let employee_id = insert_employee(pool, tenant_id).await;

    insert_tenko_session(pool, tenant_id, employee_id, Some("CARID-002")).await;
    // 車両マスタはあるが carins にまだ紐づけていない (car_id = NULL)
    let vehicle_id = insert_vehicle(pool, tenant_id, None).await;
    let category_id = insert_category(pool, tenant_id).await;
    insert_record(
        pool,
        tenant_id,
        vehicle_id,
        category_id,
        chrono::NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
    )
    .await;

    let repo = PgDriverInfoRepository::new(pool.clone());
    let records = repo
        .get_recent_maintenance_records(tenant_id, employee_id)
        .await
        .expect("query failed");

    assert!(records.is_empty());
}

/// 4. 別テナントの整備記録が混ざらない
#[tokio::test]
async fn does_not_leak_across_tenants() {
    let state = common::setup_app_state().await;
    let pool = state.pool();
    let tenant_a = common::create_test_tenant(pool, "Maintenance DriverInfo D-A").await;
    let tenant_b = common::create_test_tenant(pool, "Maintenance DriverInfo D-B").await;
    let employee_a = insert_employee(pool, tenant_a).await;

    // tenant_a の点呼セッションは CARID-SHARED を指す
    insert_tenko_session(pool, tenant_a, employee_a, Some("CARID-SHARED")).await;
    // tenant_a 側には対応する車両・記録が無い
    // tenant_b 側に同じ CarId を持つ車両と記録を仕込む (テナントをまたいで一致しないこと)
    let vehicle_b = insert_vehicle(pool, tenant_b, Some("CARID-SHARED")).await;
    let category_b = insert_category(pool, tenant_b).await;
    insert_record(
        pool,
        tenant_b,
        vehicle_b,
        category_b,
        chrono::NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
    )
    .await;

    let repo = PgDriverInfoRepository::new(pool.clone());
    let records = repo
        .get_recent_maintenance_records(tenant_a, employee_a)
        .await
        .expect("query failed");

    assert!(
        records.is_empty(),
        "tenant_a の照会に tenant_b の整備記録が混ざってはいけない"
    );
}

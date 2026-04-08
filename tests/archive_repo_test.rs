mod common;

use rust_alc_api::archive::repo::{ArchiveDb, PgArchiveDb};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

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

#[tokio::test]
async fn test_list_tables() {
    let pool = setup_pool().await;
    let db = PgArchiveDb::new(pool);

    let tables = db.list_tables("alc_api").await.unwrap();
    assert!(!tables.is_empty());
    assert!(tables.contains(&"tenants".to_string()));
}

#[tokio::test]
async fn test_fetch_columns() {
    let pool = setup_pool().await;
    let db = PgArchiveDb::new(pool);

    let cols = db.fetch_columns("alc_api", "tenants").await.unwrap();
    assert!(!cols.is_empty());
    let col_names: Vec<&str> = cols.iter().map(|(n, _, _, _)| n.as_str()).collect();
    assert!(col_names.contains(&"id"));
    assert!(col_names.contains(&"name"));
}

#[tokio::test]
async fn test_fetch_primary_key() {
    let pool = setup_pool().await;
    let db = PgArchiveDb::new(pool);

    let pk = db.fetch_primary_key("alc_api", "tenants").await.unwrap();
    assert_eq!(pk, vec!["id"]);
}

#[tokio::test]
async fn test_count_rows() {
    let pool = setup_pool().await;
    let db = PgArchiveDb::new(pool);

    let count = db.count_rows("alc_api", "tenants").await.unwrap();
    assert!(count >= 0);
}

#[tokio::test]
async fn test_fetch_rows_json() {
    let pool = setup_pool().await;
    let db = PgArchiveDb::new(pool.clone());

    // Insert a tenant to ensure at least one row
    let tenant_id = Uuid::new_v4();
    let slug = format!("test-archive-{}", &tenant_id.to_string()[..8]);
    sqlx::query(
        "INSERT INTO alc_api.tenants (id, name, slug) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
    )
    .bind(tenant_id)
    .bind("Archive Test Tenant")
    .bind(&slug)
    .execute(&pool)
    .await
    .unwrap();

    let rows = db
        .fetch_rows_json("alc_api", "tenants", 10, 0)
        .await
        .unwrap();
    assert!(!rows.is_empty());
    assert!(rows[0].get("id").is_some());

    // Offset beyond rows returns empty
    let empty = db
        .fetch_rows_json("alc_api", "tenants", 10, 999999)
        .await
        .unwrap();
    assert!(empty.is_empty());

    // Cleanup
    sqlx::query("DELETE FROM alc_api.tenants WHERE id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_dtako_lifecycle() {
    let pool = setup_pool().await;
    let db = PgArchiveDb::new(pool.clone());

    let tenant_id = Uuid::new_v4();
    let tenant_str = tenant_id.to_string();
    let slug = format!("dtako-test-{}", &tenant_str[..8]);

    // Create tenant
    sqlx::query("INSERT INTO alc_api.tenants (id, name, slug) VALUES ($1, $2, $3)")
        .bind(tenant_id)
        .bind("Dtako Test")
        .bind(&slug)
        .execute(&pool)
        .await
        .unwrap();

    // upsert_dtako_batch
    let row_json = format!(
        r#"{{"tenant_id":"{}","data_date_time":"2020-01-15 10:00:00","vehicle_cd":99,"type":"test","speed":50.0}}"#,
        tenant_str
    );
    db.upsert_dtako_batch(&[row_json.clone()]).await.unwrap();

    // list_dtako_dates
    let dates = db.list_dtako_dates().await.unwrap();
    let found = dates
        .iter()
        .any(|(tid, date, _)| tid == &tenant_str && date == "2020-01-15");
    assert!(found, "Inserted date should appear in list_dtako_dates");

    // fetch_dtako_rows_json
    let rows = db
        .fetch_dtako_rows_json(&tenant_str, "2020-01-15", 100, 0)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["vehicle_cd"], 99);

    // Offset beyond returns empty
    let empty = db
        .fetch_dtako_rows_json(&tenant_str, "2020-01-15", 100, 100)
        .await
        .unwrap();
    assert!(empty.is_empty());

    // upsert again (ON CONFLICT UPDATE)
    let row_json2 = format!(
        r#"{{"tenant_id":"{}","data_date_time":"2020-01-15 10:00:00","vehicle_cd":99,"type":"updated","speed":80.0}}"#,
        tenant_str
    );
    db.upsert_dtako_batch(&[row_json2]).await.unwrap();
    let rows2 = db
        .fetch_dtako_rows_json(&tenant_str, "2020-01-15", 100, 0)
        .await
        .unwrap();
    assert_eq!(rows2.len(), 1);
    assert_eq!(rows2[0]["speed"], 80.0);

    // Cleanup
    sqlx::query("DELETE FROM alc_api.dtakologs WHERE tenant_id = $1::UUID")
        .bind(&tenant_str)
        .execute(&pool)
        .await
        .unwrap();

    // Cleanup tenant
    sqlx::query("DELETE FROM alc_api.tenants WHERE id = $1")
        .bind(tenant_id)
        .execute(&pool)
        .await
        .unwrap();
}

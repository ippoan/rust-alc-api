use async_trait::async_trait;
use sqlx::PgPool;

pub use alc_core::repository::tenko_call::*;

pub struct PgTenkoCallRepository {
    pool: PgPool,
}

impl PgTenkoCallRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TenkoCallRepository for PgTenkoCallRepository {
    async fn register_driver(
        &self,
        call_number: &str,
        phone_number: &str,
        driver_name: &str,
        employee_code: Option<&str>,
    ) -> Result<Option<RegisterDriverResult>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        // call_number がマスタに存在するか検証
        let master = sqlx::query_as::<_, (String,)>(
            "SELECT tenant_id FROM tenko_call_numbers WHERE call_number = $1",
        )
        .bind(call_number)
        .fetch_optional(&mut *tx)
        .await?;

        let tenant_id = match master {
            Some(row) => row.0,
            None => return Ok(None),
        };

        // RLS 用にテナントをセット
        sqlx::query("SELECT set_current_tenant($1)")
            .bind(&tenant_id)
            .execute(&mut *tx)
            .await?;

        let row = sqlx::query_as::<_, (i32, Option<String>)>(
            r#"
            INSERT INTO tenko_call_drivers (phone_number, driver_name, call_number, tenant_id, employee_code, updated_at)
            VALUES ($1, $2, $3, $4, $5, now())
            ON CONFLICT (phone_number) DO UPDATE SET
                driver_name = $2, call_number = $3, tenant_id = $4, employee_code = $5, updated_at = now()
            RETURNING id, call_number
            "#,
        )
        .bind(phone_number)
        .bind(driver_name)
        .bind(call_number)
        .bind(&tenant_id)
        .bind(employee_code)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(Some(RegisterDriverResult {
            driver_id: row.0,
            call_number: row.1,
        }))
    }

    async fn record_tenko(
        &self,
        phone_number: &str,
        driver_name: &str,
        latitude: f64,
        longitude: f64,
    ) -> Result<Option<DriverInfo>, sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        // 登録済みドライバーを検索
        let driver = sqlx::query_as::<_, (i32, Option<String>, String)>(
            "SELECT id, call_number, tenant_id FROM tenko_call_drivers WHERE phone_number = $1",
        )
        .bind(phone_number)
        .fetch_optional(&mut *tx)
        .await?;

        let driver = match driver {
            Some(d) => d,
            None => return Ok(None),
        };

        // RLS 用にテナントをセット
        sqlx::query("SELECT set_current_tenant($1)")
            .bind(&driver.2)
            .execute(&mut *tx)
            .await?;

        // 位置情報ログを保存
        sqlx::query(
            r#"
            INSERT INTO tenko_call_logs (driver_id, phone_number, driver_name, latitude, longitude)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(driver.0)
        .bind(phone_number)
        .bind(driver_name)
        .bind(latitude)
        .bind(longitude)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(Some(DriverInfo {
            id: driver.0,
            call_number: driver.1,
            tenant_id: driver.2,
        }))
    }

    async fn list_numbers(&self) -> Result<Vec<TenkoCallNumberRow>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (i32, String, String, Option<String>, String)>(
            "SELECT id, call_number, tenant_id, label, created_at::text FROM tenko_call_numbers ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| TenkoCallNumberRow {
                id: r.0,
                call_number: r.1,
                tenant_id: r.2,
                label: r.3,
                created_at: r.4,
            })
            .collect())
    }

    async fn create_number(
        &self,
        call_number: &str,
        tenant_id: &str,
        label: Option<&str>,
    ) -> Result<i32, sqlx::Error> {
        let row = sqlx::query_as::<_, (i32,)>(
            "INSERT INTO tenko_call_numbers (call_number, tenant_id, label) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(call_number)
        .bind(tenant_id)
        .bind(label)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.0)
    }

    async fn delete_number(&self, id: i32) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM tenko_call_numbers WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn list_drivers(&self) -> Result<Vec<TenkoCallDriverRow>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (i32, String, String, Option<String>, String, Option<String>, String)>(
            "SELECT id, phone_number, driver_name, call_number, tenant_id, employee_code, created_at::text FROM tenko_call_drivers ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| TenkoCallDriverRow {
                id: r.0,
                phone_number: r.1,
                driver_name: r.2,
                call_number: r.3,
                tenant_id: r.4,
                employee_code: r.5,
                created_at: r.6,
            })
            .collect())
    }
}

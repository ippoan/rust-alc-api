use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::models::NfcTag;

use super::TenantConn;

#[async_trait]
pub trait NfcTagRepository: Send + Sync {
    async fn search_by_uuid(
        &self,
        tenant_id: Uuid,
        nfc_uuid: &str,
    ) -> Result<Option<NfcTag>, sqlx::Error>;

    async fn get_car_inspection_json(
        &self,
        tenant_id: Uuid,
        car_inspection_id: i32,
    ) -> Result<Option<serde_json::Value>, sqlx::Error>;

    async fn list(
        &self,
        tenant_id: Uuid,
        car_inspection_id: Option<i32>,
    ) -> Result<Vec<NfcTag>, sqlx::Error>;

    async fn register(
        &self,
        tenant_id: Uuid,
        nfc_uuid: &str,
        car_inspection_id: i32,
    ) -> Result<NfcTag, sqlx::Error>;

    async fn delete(&self, tenant_id: Uuid, nfc_uuid: &str) -> Result<bool, sqlx::Error>;
}

pub struct PgNfcTagRepository {
    pool: PgPool,
}

impl PgNfcTagRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl NfcTagRepository for PgNfcTagRepository {
    async fn search_by_uuid(
        &self,
        tenant_id: Uuid,
        nfc_uuid: &str,
    ) -> Result<Option<NfcTag>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, NfcTag>(
            "SELECT id, nfc_uuid, car_inspection_id, created_at FROM car_inspection_nfc_tags WHERE nfc_uuid = $1",
        )
        .bind(nfc_uuid)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn get_car_inspection_json(
        &self,
        tenant_id: Uuid,
        car_inspection_id: i32,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let row = sqlx::query_as::<_, (serde_json::Value,)>(
            "SELECT to_jsonb(ci) FROM car_inspection ci WHERE id = $1",
        )
        .bind(car_inspection_id)
        .fetch_optional(&mut *tc.conn)
        .await?;
        Ok(row.map(|r| r.0))
    }

    async fn list(
        &self,
        tenant_id: Uuid,
        car_inspection_id: Option<i32>,
    ) -> Result<Vec<NfcTag>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        if let Some(ci_id) = car_inspection_id {
            sqlx::query_as::<_, NfcTag>(
                "SELECT id, nfc_uuid, car_inspection_id, created_at FROM car_inspection_nfc_tags WHERE car_inspection_id = $1 ORDER BY created_at DESC",
            )
            .bind(ci_id)
            .fetch_all(&mut *tc.conn)
            .await
        } else {
            sqlx::query_as::<_, NfcTag>(
                "SELECT id, nfc_uuid, car_inspection_id, created_at FROM car_inspection_nfc_tags ORDER BY created_at DESC",
            )
            .fetch_all(&mut *tc.conn)
            .await
        }
    }

    async fn register(
        &self,
        tenant_id: Uuid,
        nfc_uuid: &str,
        car_inspection_id: i32,
    ) -> Result<NfcTag, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, NfcTag>(
            r#"
            INSERT INTO car_inspection_nfc_tags (tenant_id, nfc_uuid, car_inspection_id)
            VALUES (current_setting('app.current_tenant_id')::uuid, $1, $2)
            ON CONFLICT (tenant_id, nfc_uuid) DO UPDATE
                SET car_inspection_id = EXCLUDED.car_inspection_id,
                    created_at = NOW()
            RETURNING id, nfc_uuid, car_inspection_id, created_at
            "#,
        )
        .bind(nfc_uuid)
        .bind(car_inspection_id)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn delete(&self, tenant_id: Uuid, nfc_uuid: &str) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query("DELETE FROM car_inspection_nfc_tags WHERE nfc_uuid = $1")
            .bind(nfc_uuid)
            .execute(&mut *tc.conn)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

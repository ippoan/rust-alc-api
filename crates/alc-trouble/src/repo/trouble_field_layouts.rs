use async_trait::async_trait;
use sqlx::types::Json;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::{TroubleFieldLayout, TroubleFieldLayoutEntry};
use alc_core::tenant::TenantConn;

pub use alc_core::repository::trouble_field_layouts::*;

pub struct PgTroubleFieldLayoutsRepository {
    pool: PgPool,
}

impl PgTroubleFieldLayoutsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TroubleFieldLayoutsRepository for PgTroubleFieldLayoutsRepository {
    async fn get(&self, tenant_id: Uuid) -> Result<TroubleFieldLayout, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let row: Option<(Json<Vec<TroubleFieldLayoutEntry>>,)> =
            sqlx::query_as("SELECT settings FROM trouble_field_layouts WHERE tenant_id = $1")
                .bind(tenant_id)
                .fetch_optional(&mut *tc.conn)
                .await?;
        Ok(TroubleFieldLayout {
            settings: row.map(|(s,)| s.0).unwrap_or_default(),
        })
    }

    async fn upsert(
        &self,
        tenant_id: Uuid,
        layout: &TroubleFieldLayout,
    ) -> Result<TroubleFieldLayout, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let row: (Json<Vec<TroubleFieldLayoutEntry>>,) = sqlx::query_as(
            r#"
            INSERT INTO trouble_field_layouts (tenant_id, settings)
            VALUES ($1, $2)
            ON CONFLICT (tenant_id) DO UPDATE SET
                settings = EXCLUDED.settings,
                updated_at = NOW()
            RETURNING settings
            "#,
        )
        .bind(tenant_id)
        .bind(Json(&layout.settings))
        .fetch_one(&mut *tc.conn)
        .await?;
        Ok(TroubleFieldLayout { settings: row.0 .0 })
    }
}

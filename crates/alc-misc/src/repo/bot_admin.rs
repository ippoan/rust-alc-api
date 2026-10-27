use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::tenant::TenantConn;

pub use alc_core::repository::bot_admin::*;

pub struct PgBotAdminRepository {
    pool: PgPool,
}

impl PgBotAdminRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BotAdminRepository for PgBotAdminRepository {
    async fn get_tenant_for_export(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<TenantInfoForExport>, sqlx::Error> {
        sqlx::query_as::<_, TenantInfoForExport>(
            "SELECT id, name, slug, email_domain, created_at FROM tenants WHERE id = $1",
        )
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
    }

    async fn list_configs_for_export(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<BotConfigExportRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, BotConfigExportRow>(
            r#"
            SELECT id, tenant_id, provider, name, client_id, client_secret_encrypted,
                   service_account, bot_id, enabled,
                   bot_secret_encrypted, created_at, updated_at
            FROM bot_configs
            ORDER BY name
            "#,
        )
        .fetch_all(&mut *tc.conn)
        .await
    }
}

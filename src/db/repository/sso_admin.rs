use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use super::TenantConn;

pub use alc_core::repository::sso_admin::*;

pub struct PgSsoAdminRepository {
    pool: PgPool,
}

impl PgSsoAdminRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SsoAdminRepository for PgSsoAdminRepository {
    async fn list_configs(&self, tenant_id: Uuid) -> Result<Vec<SsoConfigRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, SsoConfigRow>(
            r#"
            SELECT provider, client_id, external_org_id, enabled, woff_id, created_at, updated_at
            FROM sso_provider_configs
            ORDER BY provider
            "#,
        )
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn upsert_config_with_secret(
        &self,
        tenant_id: Uuid,
        provider: &str,
        client_id: &str,
        client_secret_encrypted: &str,
        external_org_id: &str,
        woff_id: Option<&str>,
        enabled: bool,
    ) -> Result<SsoConfigRow, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, SsoConfigRow>(
            r#"
            INSERT INTO sso_provider_configs (tenant_id, provider, client_id, client_secret_encrypted, external_org_id, woff_id, enabled)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (tenant_id, provider) DO UPDATE SET
                client_id = EXCLUDED.client_id,
                client_secret_encrypted = EXCLUDED.client_secret_encrypted,
                external_org_id = EXCLUDED.external_org_id,
                woff_id = EXCLUDED.woff_id,
                enabled = EXCLUDED.enabled,
                updated_at = NOW()
            RETURNING provider, client_id, external_org_id, enabled, woff_id, created_at, updated_at
            "#,
        )
        .bind(tenant_id)
        .bind(provider)
        .bind(client_id)
        .bind(client_secret_encrypted)
        .bind(external_org_id)
        .bind(woff_id)
        .bind(enabled)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn upsert_config_without_secret(
        &self,
        tenant_id: Uuid,
        provider: &str,
        client_id: &str,
        external_org_id: &str,
        woff_id: Option<&str>,
        enabled: bool,
    ) -> Result<SsoConfigRow, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, SsoConfigRow>(
            r#"
            INSERT INTO sso_provider_configs (tenant_id, provider, client_id, external_org_id, woff_id, enabled)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (tenant_id, provider) DO UPDATE SET
                client_id = EXCLUDED.client_id,
                external_org_id = EXCLUDED.external_org_id,
                woff_id = EXCLUDED.woff_id,
                enabled = EXCLUDED.enabled,
                updated_at = NOW()
            RETURNING provider, client_id, external_org_id, enabled, woff_id, created_at, updated_at
            "#,
        )
        .bind(tenant_id)
        .bind(provider)
        .bind(client_id)
        .bind(external_org_id)
        .bind(woff_id)
        .bind(enabled)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn delete_config(&self, tenant_id: Uuid, provider: &str) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query("DELETE FROM sso_provider_configs WHERE tenant_id = $1 AND provider = $2")
            .bind(tenant_id)
            .bind(provider)
            .execute(&mut *tc.conn)
            .await?;
        Ok(())
    }
}

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::tenant::TenantConn;

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
        // RLS に依存せず明示的に tenant_id で絞る (defense in depth)。
        // staging の postgres sidecar は superuser 接続で RLS を bypass するため、
        // WHERE 句が無いと全テナントの config が漏れる (本番 alc_api_app は NOBYPASSRLS)。
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, SsoConfigRow>(
            r#"
            SELECT provider, client_id, external_org_id, enabled, woff_id, created_at, updated_at
            FROM sso_provider_configs
            WHERE tenant_id = $1
            ORDER BY provider
            "#,
        )
        .bind(tenant_id)
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
        // cross-tenant UNIQUE (provider, external_org_id / client_id) は「1 LW org =
        // 1 tenant」を保証する所有権境界。これを跨いで tenant_id を付け替えると別
        // tenant の config を乗っ取れてしまう (cross-tenant takeover) ため絶対にしない。
        //
        // staging の揮発 DB は tenant churn で「同じ人間の別 tenant」に orphan を残し、
        // それが cross-tenant UNIQUE に衝突して保存を 500 にする。これは staging 固有の
        // 事故なので **STAGING_MODE のときだけ** orphan を明示削除して解消する。
        // 本番 (STAGING_MODE 無効 + alc_api_app/RLS) では削除しない = tenant は永続で
        // cross-tenant 衝突は「別の実テナント」= 正当な境界。衝突時は下の INSERT が
        // UNIQUE 違反を返し、勝手な付け替え/削除はしない。
        if std::env::var("STAGING_MODE").as_deref() == Ok("true") {
            sqlx::query(
                r#"
                DELETE FROM sso_provider_configs
                WHERE provider = $1
                  AND tenant_id <> $2
                  AND (external_org_id = $3 OR client_id = $4)
                "#,
            )
            .bind(provider)
            .bind(tenant_id)
            .bind(external_org_id)
            .bind(client_id)
            .execute(&mut *tc.conn)
            .await?;
        }
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
        // secret を渡さない更新は UPDATE only。
        // client_secret_encrypted は NOT NULL なので INSERT 経路に入れられない
        // (= 旧コードは ON CONFLICT INSERT で NOT NULL 違反 500 になっていた)。
        // 既存 config が無ければ RowNotFound を返し、handler が 400 にマップする
        // (新規作成には client_secret が必須)。
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, SsoConfigRow>(
            r#"
            UPDATE sso_provider_configs SET
                client_id = $3,
                external_org_id = $4,
                woff_id = $5,
                enabled = $6,
                updated_at = NOW()
            WHERE tenant_id = $1 AND provider = $2
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

    async fn delete_config(&self, tenant_id: Uuid, provider: &str) -> Result<u64, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result =
            sqlx::query("DELETE FROM sso_provider_configs WHERE tenant_id = $1 AND provider = $2")
                .bind(tenant_id)
                .bind(provider)
                .execute(&mut *tc.conn)
                .await?;
        Ok(result.rows_affected())
    }

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
    ) -> Result<Vec<SsoConfigExportRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, SsoConfigExportRow>(
            r#"
            SELECT id, tenant_id, provider, client_id, client_secret_encrypted,
                   external_org_id, enabled, woff_id, created_at, updated_at
            FROM sso_provider_configs
            WHERE tenant_id = $1
            ORDER BY provider
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }
}

use sqlx::PgPool;

/// Set the current tenant for RLS policies.
/// Must be called before any tenant-scoped query.
pub async fn set_current_tenant(
    conn: &mut sqlx::PgConnection,
    tenant_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT set_current_tenant($1)")
        .bind(tenant_id)
        .execute(conn)
        .await?;
    Ok(())
}

/// テナントスコープの DB コネクション
/// acquire 時に set_current_tenant を自動呼び出しする
pub struct TenantConn {
    pub conn: sqlx::pool::PoolConnection<sqlx::Postgres>,
}

impl TenantConn {
    pub async fn acquire(pool: &PgPool, tenant_id: &str) -> Result<Self, sqlx::Error> {
        let mut conn = pool.acquire().await?;
        set_current_tenant(&mut conn, tenant_id).await?;
        Ok(Self { conn })
    }
}

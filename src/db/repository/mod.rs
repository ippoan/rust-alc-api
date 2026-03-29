pub mod employees;
pub mod tenko_call;
pub mod timecard;

pub use employees::{EmployeeRepository, PgEmployeeRepository};
pub use tenko_call::{PgTenkoCallRepository, TenkoCallRepository};
pub use timecard::{PgTimecardRepository, TimecardRepository};

use sqlx::PgPool;

/// テナントスコープの DB コネクション
/// acquire 時に set_current_tenant を自動呼び出しする
pub struct TenantConn {
    pub conn: sqlx::pool::PoolConnection<sqlx::Postgres>,
}

impl TenantConn {
    pub async fn acquire(pool: &PgPool, tenant_id: &str) -> Result<Self, sqlx::Error> {
        let mut conn = pool.acquire().await?;
        super::tenant::set_current_tenant(&mut conn, tenant_id).await?;
        Ok(Self { conn })
    }
}

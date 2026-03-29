pub mod car_inspections;
pub mod communication_items;
pub mod devices;
pub mod employees;
pub mod measurements;
pub mod nfc_tags;
pub mod tenko_call;
pub mod tenko_sessions;
pub mod timecard;

pub use car_inspections::{CarInspectionRepository, PgCarInspectionRepository};
pub use communication_items::{CommunicationItemsRepository, PgCommunicationItemsRepository};
pub use devices::{DeviceRepository, PgDeviceRepository};
pub use employees::{EmployeeRepository, PgEmployeeRepository};
pub use measurements::{MeasurementsRepository, PgMeasurementsRepository};
pub use nfc_tags::{NfcTagRepository, PgNfcTagRepository};
pub use tenko_call::{PgTenkoCallRepository, TenkoCallRepository};
pub use tenko_sessions::{PgTenkoSessionRepository, TenkoSessionRepository};
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

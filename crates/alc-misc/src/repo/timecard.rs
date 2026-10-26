use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::TimecardCard;

use alc_core::tenant::TenantConn;

pub use alc_core::repository::timecard::*;

/// カードの照合だけ (hub_measurements の打刻中継用。タイムカードの口は ippoan/alc-people-worker)。
pub struct PgTimecardRepository {
    pool: PgPool,
}

impl PgTimecardRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TimecardRepository for PgTimecardRepository {
    async fn find_card_by_card_id(
        &self,
        tenant_id: Uuid,
        card_id: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TimecardCard>(
            "SELECT * FROM timecard_cards WHERE tenant_id = $1 AND card_id = $2",
        )
        .bind(tenant_id)
        .bind(card_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn find_employee_id_by_nfc(
        &self,
        tenant_id: Uuid,
        nfc_id: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM employees WHERE tenant_id = $1 AND nfc_id = $2",
        )
        .bind(tenant_id)
        .bind(nfc_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }
}

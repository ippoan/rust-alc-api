use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::tenant::TenantConn;

pub use alc_core::repository::dtako_restraint_report_pdf::*;

pub struct PgDtakoRestraintReportPdfRepository {
    pool: PgPool,
}

impl PgDtakoRestraintReportPdfRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DtakoRestraintReportPdfRepository for PgDtakoRestraintReportPdfRepository {
    async fn list_drivers(&self, tenant_id: Uuid) -> Result<Vec<PdfDriver>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, PdfDriver>(
            // soft-delete 済み (重複統合で畳まれた敗者・退職者) を一覧に出さない。
            // 一方 get_driver は `id = $2` 指定なのでフィルタを足さない —
            // 足すと退職者の過去帳票が再出力できなくなる (Refs ippoan/rust-alc-api#669)。
            "SELECT id, tenant_id, driver_cd, name FROM alc_api.employees \
             WHERE tenant_id = $1 AND deleted_at IS NULL ORDER BY driver_cd",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get_driver(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
    ) -> Result<Vec<PdfDriver>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, PdfDriver>(
            "SELECT id, tenant_id, driver_cd, name FROM alc_api.employees WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant_id)
        .bind(driver_id)
        .fetch_all(&mut *tc.conn)
        .await
    }
}

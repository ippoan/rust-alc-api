use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use super::TenantConn;

pub use alc_core::repository::car_inspections::*;

pub struct PgCarInspectionRepository {
    pool: PgPool,
}

impl PgCarInspectionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CarInspectionRepository for PgCarInspectionRepository {
    async fn list_current(&self, tenant_id: Uuid) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let rows = sqlx::query_as::<_, (serde_json::Value,)>(
            r#"
            SELECT to_jsonb(sub) FROM (
                SELECT DISTINCT ON (ci."CarId")
                    ci.*,
                    (SELECT uuid::text FROM car_inspection_files_b
                     WHERE tenant_id = ci.tenant_id
                       AND "ElectCertMgNo" = ci."ElectCertMgNo"
                       AND "GrantdateE" = ci."GrantdateE"
                       AND "GrantdateY" = ci."GrantdateY"
                       AND "GrantdateM" = ci."GrantdateM"
                       AND "GrantdateD" = ci."GrantdateD"
                       AND type = 'application/pdf'
                       AND deleted_at IS NULL
                     ORDER BY created_at DESC LIMIT 1) as "pdfUuid",
                    (SELECT uuid::text FROM car_inspection_files_a
                     WHERE tenant_id = ci.tenant_id
                       AND "ElectCertMgNo" = ci."ElectCertMgNo"
                       AND "GrantdateE" = ci."GrantdateE"
                       AND "GrantdateY" = ci."GrantdateY"
                       AND "GrantdateM" = ci."GrantdateM"
                       AND "GrantdateD" = ci."GrantdateD"
                       AND type = 'application/json'
                       AND deleted_at IS NULL
                     ORDER BY created_at DESC LIMIT 1) as "jsonUuid"
                FROM car_inspection ci
                ORDER BY ci."CarId",
                         ci."TwodimensionCodeInfoValidPeriodExpirdate" DESC,
                         ci.created_at DESC
            ) sub
            "#,
        )
        .fetch_all(&mut *tc.conn)
        .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    async fn list_expired(&self, tenant_id: Uuid) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let rows = sqlx::query_as::<_, (serde_json::Value,)>(
            r#"
            SELECT to_jsonb(ci)
            FROM car_inspection ci
            WHERE "TwodimensionCodeInfoValidPeriodExpirdate" <= to_char(CURRENT_DATE + INTERVAL '30 days', 'YYMMDD')
            ORDER BY "TwodimensionCodeInfoValidPeriodExpirdate" ASC
            "#,
        )
        .fetch_all(&mut *tc.conn)
        .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    async fn list_renew(&self, tenant_id: Uuid) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let rows = sqlx::query_as::<_, (serde_json::Value,)>(
            r#"
            SELECT to_jsonb(ci)
            FROM car_inspection ci
            WHERE "TwodimensionCodeInfoValidPeriodExpirdate" >= to_char(CURRENT_DATE, 'YYMMDD')
              AND "TwodimensionCodeInfoValidPeriodExpirdate" <= to_char(CURRENT_DATE + INTERVAL '60 days', 'YYMMDD')
            ORDER BY "TwodimensionCodeInfoValidPeriodExpirdate" ASC
            "#,
        )
        .fetch_all(&mut *tc.conn)
        .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    async fn get_by_id(
        &self,
        tenant_id: Uuid,
        id: i32,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let row = sqlx::query_as::<_, (serde_json::Value,)>(
            "SELECT to_jsonb(ci) FROM car_inspection ci WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&mut *tc.conn)
        .await?;
        Ok(row.map(|r| r.0))
    }

    async fn vehicle_categories(&self, tenant_id: Uuid) -> Result<VehicleCategories, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, VehicleCategories>(
            r#"SELECT
                COALESCE(ARRAY(SELECT DISTINCT "CarKind" FROM alc_api.car_inspection WHERE "CarKind" != '' ORDER BY "CarKind"), '{}') AS car_kinds,
                COALESCE(ARRAY(SELECT DISTINCT "Use" FROM alc_api.car_inspection WHERE "Use" != '' ORDER BY "Use"), '{}') AS uses,
                COALESCE(ARRAY(SELECT DISTINCT "CarShape" FROM alc_api.car_inspection WHERE "CarShape" != '' ORDER BY "CarShape"), '{}') AS car_shapes,
                COALESCE(ARRAY(SELECT DISTINCT "PrivateBusiness" FROM alc_api.car_inspection WHERE "PrivateBusiness" != '' ORDER BY "PrivateBusiness"), '{}') AS private_businesses
            "#,
        )
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn list_current_files(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<CarInspectionFile>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, CarInspectionFile>(
            r#"
            SELECT cif.*
            FROM car_inspection_files_a cif
            INNER JOIN car_inspection ci ON
                cif."ElectCertMgNo" = ci."ElectCertMgNo"
                AND cif."GrantdateE" = ci."GrantdateE"
                AND cif."GrantdateY" = ci."GrantdateY"
                AND cif."GrantdateM" = ci."GrantdateM"
                AND cif."GrantdateD" = ci."GrantdateD"
            WHERE cif.deleted_at IS NULL
              AND ci."TwodimensionCodeInfoValidPeriodExpirdate" >= to_char(CURRENT_DATE, 'YYMMDD')
            ORDER BY cif.created_at DESC
            "#,
        )
        .fetch_all(&mut *tc.conn)
        .await
    }
}

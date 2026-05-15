use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::VehicleSettingsDump;
use alc_core::tenant::TenantConn;

pub use alc_core::repository::vehicle_settings_dumps::*;

pub struct PgVehicleSettingsDumpsRepository {
    pool: PgPool,
}

impl PgVehicleSettingsDumpsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl VehicleSettingsDumpsRepository for PgVehicleSettingsDumpsRepository {
    async fn register(
        &self,
        tenant_id: Uuid,
        input: VehicleSettingsDumpInput,
    ) -> Result<VehicleSettingsDump, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        // ON CONFLICT で処理 — 同じ zip を 2 回投げたケースや R2 put 後の retry に保証をもたせる。
        // 上書きしたい (machine_id が達う 等) ので DO UPDATE を使う。
        sqlx::query_as::<_, VehicleSettingsDump>(
            r#"
            INSERT INTO alc_api.vehicle_settings_dumps
              (tenant_id, vehicle_cd, dump_dir, machine_id, firm_main_app,
               r2_json_key, r2_cfg_key, uploaded_by)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (tenant_id, vehicle_cd, dump_dir) DO UPDATE
              SET machine_id    = EXCLUDED.machine_id,
                  firm_main_app = EXCLUDED.firm_main_app,
                  r2_json_key   = EXCLUDED.r2_json_key,
                  r2_cfg_key    = EXCLUDED.r2_cfg_key,
                  uploaded_by   = COALESCE(EXCLUDED.uploaded_by, alc_api.vehicle_settings_dumps.uploaded_by),
                  uploaded_at   = now()
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(&input.vehicle_cd)
        .bind(&input.dump_dir)
        .bind(input.machine_id.as_deref())
        .bind(input.firm_main_app.as_deref())
        .bind(&input.r2_json_key)
        .bind(&input.r2_cfg_key)
        .bind(input.uploaded_by)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn list_by_vehicle_cd(
        &self,
        tenant_id: Uuid,
        vehicle_cd: &str,
    ) -> Result<Vec<VehicleSettingsDump>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, VehicleSettingsDump>(
            "SELECT * FROM alc_api.vehicle_settings_dumps \
             WHERE tenant_id = $1 AND vehicle_cd = $2 \
             ORDER BY uploaded_at DESC",
        )
        .bind(tenant_id)
        .bind(vehicle_cd)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn summary_by_vehicle(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<VehicleSettingsDumpSummary>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, VehicleSettingsDumpSummary>(
            "SELECT vehicle_cd, \
                    COUNT(*)::BIGINT  AS count, \
                    MAX(uploaded_at)  AS latest_uploaded_at \
             FROM alc_api.vehicle_settings_dumps \
             WHERE tenant_id = $1 \
             GROUP BY vehicle_cd \
             ORDER BY vehicle_cd",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn confirmed_vehicle_cds(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<String>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT vehicle_cd \
             FROM alc_api.vehicle_settings_dumps \
             WHERE tenant_id = $1",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await?;
        Ok(rows.into_iter().map(|(cd,)| cd).collect())
    }
}

// VehicleSettingsDumpSummary は serde デリーブだけだと query_as に使えないので
// ここで FromRow を手動実装する。trait 側に記述すると alc-core に sqlx 依存が漏れるため。
impl<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> for VehicleSettingsDumpSummary {
    fn from_row(row: &'r sqlx::postgres::PgRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        Ok(VehicleSettingsDumpSummary {
            vehicle_cd: row.try_get("vehicle_cd")?,
            count: row.try_get("count")?,
            latest_uploaded_at: row.try_get("latest_uploaded_at")?,
        })
    }
}

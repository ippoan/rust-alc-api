use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::DtakologRow;
use alc_core::tenant::TenantConn;

pub use alc_core::repository::dtako_logs::*;

pub struct PgDtakoLogsRepository {
    pool: PgPool,
}

impl PgDtakoLogsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const SELECT_COLS: &str = r#"
    gps_direction, gps_latitude, gps_longitude, vehicle_cd,
    vehicle_name, driver_name, address_disp_c, data_date_time,
    address_disp_p, sub_driver_cd, all_state, recive_type_color_name,
    all_state_ex, state2, all_state_font_color, speed
"#;

#[async_trait]
impl DtakoLogsRepository for PgDtakoLogsRepository {
    async fn current_list_all(&self, tenant_id: Uuid) -> Result<Vec<DtakologRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let sql = format!(
            r#"SELECT {SELECT_COLS}
               FROM alc_api.dtakologs d
               INNER JOIN (
                   SELECT vehicle_cd, MAX(data_date_time) AS max_dt
                   FROM alc_api.dtakologs
                   GROUP BY vehicle_cd
               ) latest ON d.vehicle_cd = latest.vehicle_cd
                       AND d.data_date_time = latest.max_dt
               ORDER BY d.data_date_time DESC"#
        );
        sqlx::query_as::<_, DtakologRow>(&sql)
            .fetch_all(&mut *tc.conn)
            .await
    }

    async fn get_date(
        &self,
        tenant_id: Uuid,
        date_time: &str,
        vehicle_cd: Option<i32>,
    ) -> Result<Vec<DtakologRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        if let Some(vc) = vehicle_cd {
            let sql = format!(
                r#"SELECT {SELECT_COLS}
                   FROM alc_api.dtakologs
                   WHERE data_date_time = $1 AND vehicle_cd = $2
                   ORDER BY data_date_time DESC"#
            );
            sqlx::query_as::<_, DtakologRow>(&sql)
                .bind(date_time)
                .bind(vc)
                .fetch_all(&mut *tc.conn)
                .await
        } else {
            let sql = format!(
                r#"SELECT {SELECT_COLS}
                   FROM alc_api.dtakologs
                   WHERE data_date_time = $1
                   ORDER BY data_date_time DESC"#
            );
            sqlx::query_as::<_, DtakologRow>(&sql)
                .bind(date_time)
                .fetch_all(&mut *tc.conn)
                .await
        }
    }

    async fn current_list_select(
        &self,
        tenant_id: Uuid,
        address_disp_p: Option<&str>,
        branch_cd: Option<i32>,
        vehicle_cds: &[i32],
    ) -> Result<Vec<DtakologRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let sql = format!(
            r#"SELECT {SELECT_COLS}
               FROM alc_api.dtakologs d
               INNER JOIN (
                   SELECT vehicle_cd, MAX(data_date_time) AS max_dt
                   FROM alc_api.dtakologs
                   GROUP BY vehicle_cd
               ) latest ON d.vehicle_cd = latest.vehicle_cd
                       AND d.data_date_time = latest.max_dt
               WHERE ($1::TEXT IS NULL OR d.address_disp_p = $1)
                 AND ($2::INTEGER IS NULL OR d.branch_cd = $2)
                 AND ($3::INTEGER[] IS NULL OR array_length($3, 1) IS NULL OR d.vehicle_cd = ANY($3))
               ORDER BY d.data_date_time DESC"#
        );
        let vehicle_cds_param: Option<&[i32]> = if vehicle_cds.is_empty() {
            None
        } else {
            Some(vehicle_cds)
        };
        sqlx::query_as::<_, DtakologRow>(&sql)
            .bind(address_disp_p)
            .bind(branch_cd)
            .bind(vehicle_cds_param)
            .fetch_all(&mut *tc.conn)
            .await
    }

    async fn get_date_range(
        &self,
        tenant_id: Uuid,
        start: &str,
        end: &str,
        vehicle_cd: Option<i32>,
    ) -> Result<Vec<DtakologRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let sql = format!(
            r#"SELECT {SELECT_COLS}
               FROM alc_api.dtakologs
               WHERE data_date_time >= $1 AND data_date_time <= $2
                 AND ($3::INTEGER IS NULL OR vehicle_cd = $3)
               ORDER BY data_date_time DESC"#
        );
        sqlx::query_as::<_, DtakologRow>(&sql)
            .bind(start)
            .bind(end)
            .bind(vehicle_cd)
            .fetch_all(&mut *tc.conn)
            .await
    }
}

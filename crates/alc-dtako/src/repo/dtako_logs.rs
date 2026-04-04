use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::{DtakologInput, DtakologRow};
use alc_core::tenant::TenantConn;

pub use alc_core::repository::dtako_logs::*;

const CHUNK_SIZE: usize = 500;

pub struct PgDtakoLogsRepository {
    pool: PgPool,
}

impl PgDtakoLogsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const SELECT_COLS: &str = r#"
    d.gps_direction, d.gps_latitude, d.gps_longitude, d.vehicle_cd,
    d.vehicle_name, d.driver_name, d.address_disp_c, d.data_date_time,
    d.address_disp_p, d.sub_driver_cd, d.all_state, d.recive_type_color_name,
    d.all_state_ex, d.state2, d.all_state_font_color, d.speed
"#;

const SELECT_COLS_SIMPLE: &str = r#"
    gps_direction, gps_latitude, gps_longitude, vehicle_cd,
    vehicle_name, driver_name, address_disp_c, data_date_time,
    address_disp_p, sub_driver_cd, all_state, recive_type_color_name,
    all_state_ex, state2, all_state_font_color, speed
"#;

#[async_trait]
impl DtakoLogsRepository for PgDtakoLogsRepository {
    async fn bulk_upsert(
        &self,
        tenant_id: Uuid,
        records: &[DtakologInput],
    ) -> Result<u64, sqlx::Error> {
        if records.is_empty() {
            return Ok(0);
        }
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let mut total_affected: u64 = 0;

        for chunk in records.chunks(CHUNK_SIZE) {
            let mut data_date_time: Vec<&str> = Vec::with_capacity(chunk.len());
            let mut vehicle_cd: Vec<i32> = Vec::with_capacity(chunk.len());
            let mut r#type: Vec<&str> = Vec::with_capacity(chunk.len());
            let mut all_state_font_color_index: Vec<i32> = Vec::with_capacity(chunk.len());
            let mut all_state_ryout_color: Vec<&str> = Vec::with_capacity(chunk.len());
            let mut branch_cd: Vec<i32> = Vec::with_capacity(chunk.len());
            let mut branch_name: Vec<&str> = Vec::with_capacity(chunk.len());
            let mut current_work_cd: Vec<i32> = Vec::with_capacity(chunk.len());
            let mut data_filter_type: Vec<i32> = Vec::with_capacity(chunk.len());
            let mut disp_flag: Vec<i32> = Vec::with_capacity(chunk.len());
            let mut driver_cd: Vec<i32> = Vec::with_capacity(chunk.len());
            let mut gps_direction: Vec<i32> = Vec::with_capacity(chunk.len());
            let mut gps_enable: Vec<i32> = Vec::with_capacity(chunk.len());
            let mut gps_latitude: Vec<i32> = Vec::with_capacity(chunk.len());
            let mut gps_longitude: Vec<i32> = Vec::with_capacity(chunk.len());
            let mut gps_satellite_num: Vec<i32> = Vec::with_capacity(chunk.len());
            let mut operation_state: Vec<i32> = Vec::with_capacity(chunk.len());
            let mut recive_event_type: Vec<i32> = Vec::with_capacity(chunk.len());
            let mut recive_packet_type: Vec<i32> = Vec::with_capacity(chunk.len());
            let mut recive_work_cd: Vec<i32> = Vec::with_capacity(chunk.len());
            let mut revo: Vec<i32> = Vec::with_capacity(chunk.len());
            let mut setting_temp: Vec<&str> = Vec::with_capacity(chunk.len());
            let mut setting_temp1: Vec<&str> = Vec::with_capacity(chunk.len());
            let mut setting_temp3: Vec<&str> = Vec::with_capacity(chunk.len());
            let mut setting_temp4: Vec<&str> = Vec::with_capacity(chunk.len());
            let mut speed: Vec<f32> = Vec::with_capacity(chunk.len());
            let mut sub_driver_cd: Vec<i32> = Vec::with_capacity(chunk.len());
            let mut temp_state: Vec<i32> = Vec::with_capacity(chunk.len());
            let mut vehicle_name: Vec<&str> = Vec::with_capacity(chunk.len());
            let mut address_disp_c: Vec<Option<&str>> = Vec::with_capacity(chunk.len());
            let mut address_disp_p: Vec<Option<&str>> = Vec::with_capacity(chunk.len());
            let mut all_state: Vec<Option<&str>> = Vec::with_capacity(chunk.len());
            let mut all_state_ex: Vec<Option<&str>> = Vec::with_capacity(chunk.len());
            let mut all_state_font_color: Vec<Option<&str>> = Vec::with_capacity(chunk.len());
            let mut comu_date_time: Vec<Option<&str>> = Vec::with_capacity(chunk.len());
            let mut current_work_name: Vec<Option<&str>> = Vec::with_capacity(chunk.len());
            let mut driver_name: Vec<Option<&str>> = Vec::with_capacity(chunk.len());
            let mut event_val: Vec<Option<&str>> = Vec::with_capacity(chunk.len());
            let mut gps_lati_and_long: Vec<Option<&str>> = Vec::with_capacity(chunk.len());
            let mut odometer: Vec<Option<&str>> = Vec::with_capacity(chunk.len());
            let mut recive_type_color_name: Vec<Option<&str>> = Vec::with_capacity(chunk.len());
            let mut recive_type_name: Vec<Option<&str>> = Vec::with_capacity(chunk.len());
            let mut start_work_date_time: Vec<Option<&str>> = Vec::with_capacity(chunk.len());
            let mut state: Vec<Option<&str>> = Vec::with_capacity(chunk.len());
            let mut state1: Vec<Option<&str>> = Vec::with_capacity(chunk.len());
            let mut state2: Vec<Option<&str>> = Vec::with_capacity(chunk.len());
            let mut state3: Vec<Option<&str>> = Vec::with_capacity(chunk.len());
            let mut state_flag: Vec<Option<&str>> = Vec::with_capacity(chunk.len());
            let mut temp1: Vec<Option<&str>> = Vec::with_capacity(chunk.len());
            let mut temp2: Vec<Option<&str>> = Vec::with_capacity(chunk.len());
            let mut temp3: Vec<Option<&str>> = Vec::with_capacity(chunk.len());
            let mut temp4: Vec<Option<&str>> = Vec::with_capacity(chunk.len());
            let mut vehicle_icon_color: Vec<Option<&str>> = Vec::with_capacity(chunk.len());
            let mut vehicle_icon_label_for_datetime: Vec<Option<&str>> =
                Vec::with_capacity(chunk.len());
            let mut vehicle_icon_label_for_driver: Vec<Option<&str>> =
                Vec::with_capacity(chunk.len());
            let mut vehicle_icon_label_for_vehicle: Vec<Option<&str>> =
                Vec::with_capacity(chunk.len());

            for r in chunk {
                data_date_time.push(&r.data_date_time);
                vehicle_cd.push(r.vehicle_cd);
                r#type.push(&r.r#type);
                all_state_font_color_index.push(r.all_state_font_color_index);
                all_state_ryout_color.push(&r.all_state_ryout_color);
                branch_cd.push(r.branch_cd);
                branch_name.push(&r.branch_name);
                current_work_cd.push(r.current_work_cd);
                data_filter_type.push(r.data_filter_type);
                disp_flag.push(r.disp_flag);
                driver_cd.push(r.driver_cd);
                gps_direction.push(r.gps_direction);
                gps_enable.push(r.gps_enable);
                gps_latitude.push(r.gps_latitude);
                gps_longitude.push(r.gps_longitude);
                gps_satellite_num.push(r.gps_satellite_num);
                operation_state.push(r.operation_state);
                recive_event_type.push(r.recive_event_type);
                recive_packet_type.push(r.recive_packet_type);
                recive_work_cd.push(r.recive_work_cd);
                revo.push(r.revo);
                setting_temp.push(&r.setting_temp);
                setting_temp1.push(&r.setting_temp1);
                setting_temp3.push(&r.setting_temp3);
                setting_temp4.push(&r.setting_temp4);
                speed.push(r.speed);
                sub_driver_cd.push(r.sub_driver_cd);
                temp_state.push(r.temp_state);
                vehicle_name.push(&r.vehicle_name);
                address_disp_c.push(r.address_disp_c.as_deref());
                address_disp_p.push(r.address_disp_p.as_deref());
                all_state.push(r.all_state.as_deref());
                all_state_ex.push(r.all_state_ex.as_deref());
                all_state_font_color.push(r.all_state_font_color.as_deref());
                comu_date_time.push(r.comu_date_time.as_deref());
                current_work_name.push(r.current_work_name.as_deref());
                driver_name.push(r.driver_name.as_deref());
                event_val.push(r.event_val.as_deref());
                gps_lati_and_long.push(r.gps_lati_and_long.as_deref());
                odometer.push(r.odometer.as_deref());
                recive_type_color_name.push(r.recive_type_color_name.as_deref());
                recive_type_name.push(r.recive_type_name.as_deref());
                start_work_date_time.push(r.start_work_date_time.as_deref());
                state.push(r.state.as_deref());
                state1.push(r.state1.as_deref());
                state2.push(r.state2.as_deref());
                state3.push(r.state3.as_deref());
                state_flag.push(r.state_flag.as_deref());
                temp1.push(r.temp1.as_deref());
                temp2.push(r.temp2.as_deref());
                temp3.push(r.temp3.as_deref());
                temp4.push(r.temp4.as_deref());
                vehicle_icon_color.push(r.vehicle_icon_color.as_deref());
                vehicle_icon_label_for_datetime.push(r.vehicle_icon_label_for_datetime.as_deref());
                vehicle_icon_label_for_driver.push(r.vehicle_icon_label_for_driver.as_deref());
                vehicle_icon_label_for_vehicle.push(r.vehicle_icon_label_for_vehicle.as_deref());
            }

            let result = sqlx::query(
                r#"INSERT INTO alc_api.dtakologs (
                    tenant_id, data_date_time, vehicle_cd,
                    type, all_state_font_color_index, all_state_ryout_color,
                    branch_cd, branch_name, current_work_cd, data_filter_type,
                    disp_flag, driver_cd, gps_direction, gps_enable,
                    gps_latitude, gps_longitude, gps_satellite_num,
                    operation_state, recive_event_type, recive_packet_type,
                    recive_work_cd, revo, setting_temp, setting_temp1,
                    setting_temp3, setting_temp4, speed, sub_driver_cd,
                    temp_state, vehicle_name,
                    address_disp_c, address_disp_p, all_state, all_state_ex,
                    all_state_font_color, comu_date_time, current_work_name,
                    driver_name, event_val, gps_lati_and_long, odometer,
                    recive_type_color_name, recive_type_name,
                    start_work_date_time, state, state1, state2, state3,
                    state_flag, temp1, temp2, temp3, temp4,
                    vehicle_icon_color, vehicle_icon_label_for_datetime,
                    vehicle_icon_label_for_driver, vehicle_icon_label_for_vehicle
                )
                SELECT
                    $1::UUID,
                    UNNEST($2::TEXT[]), UNNEST($3::INTEGER[]),
                    UNNEST($4::TEXT[]), UNNEST($5::INTEGER[]), UNNEST($6::TEXT[]),
                    UNNEST($7::INTEGER[]), UNNEST($8::TEXT[]), UNNEST($9::INTEGER[]), UNNEST($10::INTEGER[]),
                    UNNEST($11::INTEGER[]), UNNEST($12::INTEGER[]), UNNEST($13::INTEGER[]), UNNEST($14::INTEGER[]),
                    UNNEST($15::INTEGER[]), UNNEST($16::INTEGER[]), UNNEST($17::INTEGER[]),
                    UNNEST($18::INTEGER[]), UNNEST($19::INTEGER[]), UNNEST($20::INTEGER[]),
                    UNNEST($21::INTEGER[]), UNNEST($22::INTEGER[]), UNNEST($23::TEXT[]), UNNEST($24::TEXT[]),
                    UNNEST($25::TEXT[]), UNNEST($26::TEXT[]), UNNEST($27::REAL[]), UNNEST($28::INTEGER[]),
                    UNNEST($29::INTEGER[]), UNNEST($30::TEXT[]),
                    UNNEST($31::TEXT[]), UNNEST($32::TEXT[]), UNNEST($33::TEXT[]), UNNEST($34::TEXT[]),
                    UNNEST($35::TEXT[]), UNNEST($36::TEXT[]), UNNEST($37::TEXT[]),
                    UNNEST($38::TEXT[]), UNNEST($39::TEXT[]), UNNEST($40::TEXT[]), UNNEST($41::TEXT[]),
                    UNNEST($42::TEXT[]), UNNEST($43::TEXT[]),
                    UNNEST($44::TEXT[]), UNNEST($45::TEXT[]), UNNEST($46::TEXT[]), UNNEST($47::TEXT[]), UNNEST($48::TEXT[]),
                    UNNEST($49::TEXT[]), UNNEST($50::TEXT[]), UNNEST($51::TEXT[]), UNNEST($52::TEXT[]), UNNEST($53::TEXT[]),
                    UNNEST($54::TEXT[]), UNNEST($55::TEXT[]),
                    UNNEST($56::TEXT[]), UNNEST($57::TEXT[])
                ON CONFLICT (tenant_id, data_date_time, vehicle_cd) DO UPDATE SET
                    type = EXCLUDED.type,
                    all_state_font_color_index = EXCLUDED.all_state_font_color_index,
                    all_state_ryout_color = EXCLUDED.all_state_ryout_color,
                    branch_cd = EXCLUDED.branch_cd,
                    branch_name = EXCLUDED.branch_name,
                    current_work_cd = EXCLUDED.current_work_cd,
                    data_filter_type = EXCLUDED.data_filter_type,
                    disp_flag = EXCLUDED.disp_flag,
                    driver_cd = EXCLUDED.driver_cd,
                    gps_direction = EXCLUDED.gps_direction,
                    gps_enable = EXCLUDED.gps_enable,
                    gps_latitude = EXCLUDED.gps_latitude,
                    gps_longitude = EXCLUDED.gps_longitude,
                    gps_satellite_num = EXCLUDED.gps_satellite_num,
                    operation_state = EXCLUDED.operation_state,
                    recive_event_type = EXCLUDED.recive_event_type,
                    recive_packet_type = EXCLUDED.recive_packet_type,
                    recive_work_cd = EXCLUDED.recive_work_cd,
                    revo = EXCLUDED.revo,
                    setting_temp = EXCLUDED.setting_temp,
                    setting_temp1 = EXCLUDED.setting_temp1,
                    setting_temp3 = EXCLUDED.setting_temp3,
                    setting_temp4 = EXCLUDED.setting_temp4,
                    speed = EXCLUDED.speed,
                    sub_driver_cd = EXCLUDED.sub_driver_cd,
                    temp_state = EXCLUDED.temp_state,
                    vehicle_name = EXCLUDED.vehicle_name,
                    address_disp_c = EXCLUDED.address_disp_c,
                    address_disp_p = EXCLUDED.address_disp_p,
                    all_state = EXCLUDED.all_state,
                    all_state_ex = EXCLUDED.all_state_ex,
                    all_state_font_color = EXCLUDED.all_state_font_color,
                    comu_date_time = EXCLUDED.comu_date_time,
                    current_work_name = EXCLUDED.current_work_name,
                    driver_name = EXCLUDED.driver_name,
                    event_val = EXCLUDED.event_val,
                    gps_lati_and_long = EXCLUDED.gps_lati_and_long,
                    odometer = EXCLUDED.odometer,
                    recive_type_color_name = EXCLUDED.recive_type_color_name,
                    recive_type_name = EXCLUDED.recive_type_name,
                    start_work_date_time = EXCLUDED.start_work_date_time,
                    state = EXCLUDED.state,
                    state1 = EXCLUDED.state1,
                    state2 = EXCLUDED.state2,
                    state3 = EXCLUDED.state3,
                    state_flag = EXCLUDED.state_flag,
                    temp1 = EXCLUDED.temp1,
                    temp2 = EXCLUDED.temp2,
                    temp3 = EXCLUDED.temp3,
                    temp4 = EXCLUDED.temp4,
                    vehicle_icon_color = EXCLUDED.vehicle_icon_color,
                    vehicle_icon_label_for_datetime = EXCLUDED.vehicle_icon_label_for_datetime,
                    vehicle_icon_label_for_driver = EXCLUDED.vehicle_icon_label_for_driver,
                    vehicle_icon_label_for_vehicle = EXCLUDED.vehicle_icon_label_for_vehicle
                "#,
            )
            .bind(tenant_id)
            .bind(&data_date_time)
            .bind(&vehicle_cd)
            .bind(&r#type)
            .bind(&all_state_font_color_index)
            .bind(&all_state_ryout_color)
            .bind(&branch_cd)
            .bind(&branch_name)
            .bind(&current_work_cd)
            .bind(&data_filter_type)
            .bind(&disp_flag)
            .bind(&driver_cd)
            .bind(&gps_direction)
            .bind(&gps_enable)
            .bind(&gps_latitude)
            .bind(&gps_longitude)
            .bind(&gps_satellite_num)
            .bind(&operation_state)
            .bind(&recive_event_type)
            .bind(&recive_packet_type)
            .bind(&recive_work_cd)
            .bind(&revo)
            .bind(&setting_temp)
            .bind(&setting_temp1)
            .bind(&setting_temp3)
            .bind(&setting_temp4)
            .bind(&speed)
            .bind(&sub_driver_cd)
            .bind(&temp_state)
            .bind(&vehicle_name)
            .bind(&address_disp_c)
            .bind(&address_disp_p)
            .bind(&all_state)
            .bind(&all_state_ex)
            .bind(&all_state_font_color)
            .bind(&comu_date_time)
            .bind(&current_work_name)
            .bind(&driver_name)
            .bind(&event_val)
            .bind(&gps_lati_and_long)
            .bind(&odometer)
            .bind(&recive_type_color_name)
            .bind(&recive_type_name)
            .bind(&start_work_date_time)
            .bind(&state)
            .bind(&state1)
            .bind(&state2)
            .bind(&state3)
            .bind(&state_flag)
            .bind(&temp1)
            .bind(&temp2)
            .bind(&temp3)
            .bind(&temp4)
            .bind(&vehicle_icon_color)
            .bind(&vehicle_icon_label_for_datetime)
            .bind(&vehicle_icon_label_for_driver)
            .bind(&vehicle_icon_label_for_vehicle)
            .execute(&mut *tc.conn)
            .await?;

            total_affected += result.rows_affected();
        }

        Ok(total_affected)
    }

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
                r#"SELECT {SELECT_COLS_SIMPLE}
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
                r#"SELECT {SELECT_COLS_SIMPLE}
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
            r#"SELECT {SELECT_COLS_SIMPLE}
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

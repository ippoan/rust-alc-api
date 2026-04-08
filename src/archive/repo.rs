use async_trait::async_trait;

/// DB 操作を抽象化。テストでは MockArchiveDb で差し替え可能。
#[async_trait]
pub trait ArchiveDb: Send + Sync {
    // schema_meta
    async fn fetch_columns(
        &self,
        schema: &str,
        table: &str,
    ) -> anyhow::Result<Vec<(String, String, String, Option<String>)>>;
    async fn fetch_primary_key(&self, schema: &str, table: &str) -> anyhow::Result<Vec<String>>;

    // logi
    async fn list_tables(&self, schema: &str) -> anyhow::Result<Vec<String>>;
    async fn count_rows(&self, schema: &str, table: &str) -> anyhow::Result<i64>;
    async fn fetch_rows_json(
        &self,
        schema: &str,
        table: &str,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<serde_json::Value>>;

    // dtako archive
    async fn list_dtako_dates(&self) -> anyhow::Result<Vec<(String, String, i64)>>;
    async fn fetch_dtako_rows_json(
        &self,
        tenant_id: &str,
        date: &str,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<serde_json::Value>>;
    async fn list_old_dtako_dates(
        &self,
        cutoff: &str,
    ) -> anyhow::Result<Vec<(String, String, i64)>>;
    async fn delete_dtako_date(&self, tenant_id: &str, date: &str) -> anyhow::Result<u64>;

    // dtako restore
    async fn upsert_dtako_batch(&self, rows_json: &[String]) -> anyhow::Result<()>;
}

/// PostgreSQL 実装
pub struct PgArchiveDb {
    pool: sqlx::PgPool,
}

impl PgArchiveDb {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ArchiveDb for PgArchiveDb {
    async fn fetch_columns(
        &self,
        schema: &str,
        table: &str,
    ) -> anyhow::Result<Vec<(String, String, String, Option<String>)>> {
        let rows = sqlx::query_as::<_, (String, String, String, Option<String>)>(
            "SELECT column_name, data_type, is_nullable, column_default
             FROM information_schema.columns
             WHERE table_schema = $1 AND table_name = $2
             ORDER BY ordinal_position",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn fetch_primary_key(&self, schema: &str, table: &str) -> anyhow::Result<Vec<String>> {
        let rows = sqlx::query_as::<_, (String,)>(
            "SELECT a.attname
             FROM pg_index i
             JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
             WHERE i.indrelid = ($1 || '.' || $2)::regclass AND i.indisprimary
             ORDER BY array_position(i.indkey, a.attnum)",
        )
        .bind(schema)
        .bind(table)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(n,)| n).collect())
    }

    async fn list_tables(&self, schema: &str) -> anyhow::Result<Vec<String>> {
        let rows = sqlx::query_as::<_, (String,)>(
            "SELECT tablename FROM pg_tables WHERE schemaname = $1 ORDER BY tablename",
        )
        .bind(schema)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(n,)| n).collect())
    }

    async fn count_rows(&self, schema: &str, table: &str) -> anyhow::Result<i64> {
        let row: (i64,) = sqlx::query_as(&format!(
            "SELECT count(*) FROM \"{}\".\"{}\"",
            schema, table
        ))
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    async fn fetch_rows_json(
        &self,
        schema: &str,
        table: &str,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<serde_json::Value>> {
        let rows: Vec<(serde_json::Value,)> = sqlx::query_as(&format!(
            "SELECT row_to_json(t) FROM \"{}\".\"{}\" t ORDER BY ctid LIMIT {} OFFSET {}",
            schema, table, limit, offset
        ))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(v,)| v).collect())
    }

    async fn list_dtako_dates(&self) -> anyhow::Result<Vec<(String, String, i64)>> {
        let rows = sqlx::query_as::<_, (String, String, i64)>(
            "SELECT tenant_id::TEXT, data_date_time::DATE::TEXT AS date_str, COUNT(*)
             FROM alc_api.dtakologs
             GROUP BY tenant_id, data_date_time::DATE
             ORDER BY date_str",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn fetch_dtako_rows_json(
        &self,
        tenant_id: &str,
        date: &str,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<serde_json::Value>> {
        let rows: Vec<(serde_json::Value,)> = sqlx::query_as(
            "SELECT row_to_json(d) FROM alc_api.dtakologs d
             WHERE tenant_id = $1::UUID AND data_date_time::DATE = $2::DATE
             ORDER BY data_date_time
             LIMIT $3 OFFSET $4",
        )
        .bind(tenant_id)
        .bind(date)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(v,)| v).collect())
    }

    async fn list_old_dtako_dates(
        &self,
        cutoff: &str,
    ) -> anyhow::Result<Vec<(String, String, i64)>> {
        let rows = sqlx::query_as::<_, (String, String, i64)>(
            "SELECT tenant_id::TEXT, data_date_time::DATE::TEXT AS date_str, COUNT(*)
             FROM alc_api.dtakologs
             WHERE data_date_time::DATE < $1::DATE
             GROUP BY tenant_id, data_date_time::DATE
             ORDER BY date_str",
        )
        .bind(cutoff)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn delete_dtako_date(&self, tenant_id: &str, date: &str) -> anyhow::Result<u64> {
        let result = sqlx::query(
            "DELETE FROM alc_api.dtakologs
             WHERE tenant_id = $1::UUID AND data_date_time::DATE = $2::DATE",
        )
        .bind(tenant_id)
        .bind(date)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    async fn upsert_dtako_batch(&self, rows_json: &[String]) -> anyhow::Result<()> {
        for row_json in rows_json {
            let v: serde_json::Value = serde_json::from_str(row_json)?;
            sqlx::query(
                "INSERT INTO alc_api.dtakologs (
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
                    (j->>'tenant_id')::UUID, j->>'data_date_time', (j->>'vehicle_cd')::INTEGER,
                    COALESCE(j->>'type', ''),
                    COALESCE((j->>'all_state_font_color_index')::INTEGER, 0),
                    COALESCE(j->>'all_state_ryout_color', 'Transparent'),
                    COALESCE((j->>'branch_cd')::INTEGER, 0), COALESCE(j->>'branch_name', ''),
                    COALESCE((j->>'current_work_cd')::INTEGER, 0),
                    COALESCE((j->>'data_filter_type')::INTEGER, 0),
                    COALESCE((j->>'disp_flag')::INTEGER, 0), COALESCE((j->>'driver_cd')::INTEGER, 0),
                    COALESCE((j->>'gps_direction')::DOUBLE PRECISION, 0),
                    COALESCE((j->>'gps_enable')::INTEGER, 0),
                    COALESCE((j->>'gps_latitude')::DOUBLE PRECISION, 0),
                    COALESCE((j->>'gps_longitude')::DOUBLE PRECISION, 0),
                    COALESCE((j->>'gps_satellite_num')::INTEGER, 0),
                    COALESCE((j->>'operation_state')::INTEGER, 0),
                    COALESCE((j->>'recive_event_type')::INTEGER, 0),
                    COALESCE((j->>'recive_packet_type')::INTEGER, 0),
                    COALESCE((j->>'recive_work_cd')::INTEGER, 0),
                    COALESCE((j->>'revo')::INTEGER, 0),
                    COALESCE(j->>'setting_temp', ''), COALESCE(j->>'setting_temp1', ''),
                    COALESCE(j->>'setting_temp3', ''), COALESCE(j->>'setting_temp4', ''),
                    COALESCE((j->>'speed')::REAL, 0), COALESCE((j->>'sub_driver_cd')::INTEGER, 0),
                    COALESCE((j->>'temp_state')::INTEGER, 0), COALESCE(j->>'vehicle_name', ''),
                    j->>'address_disp_c', j->>'address_disp_p', j->>'all_state', j->>'all_state_ex',
                    j->>'all_state_font_color', j->>'comu_date_time', j->>'current_work_name',
                    j->>'driver_name', j->>'event_val', j->>'gps_lati_and_long', j->>'odometer',
                    j->>'recive_type_color_name', j->>'recive_type_name',
                    j->>'start_work_date_time', j->>'state', j->>'state1',
                    j->>'state2', j->>'state3', j->>'state_flag',
                    j->>'temp1', j->>'temp2', j->>'temp3', j->>'temp4',
                    j->>'vehicle_icon_color', j->>'vehicle_icon_label_for_datetime',
                    j->>'vehicle_icon_label_for_driver', j->>'vehicle_icon_label_for_vehicle'
                FROM jsonb_array_elements($1::JSONB) AS j
                ON CONFLICT (tenant_id, data_date_time, vehicle_cd) DO UPDATE SET
                    type = EXCLUDED.type, speed = EXCLUDED.speed,
                    gps_latitude = EXCLUDED.gps_latitude, gps_longitude = EXCLUDED.gps_longitude,
                    gps_direction = EXCLUDED.gps_direction",
            )
            .bind(format!("[{}]", v))
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }
}

/// テスト用 mock
#[cfg(test)]
pub mod mock {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    pub struct MockArchiveDb {
        pub fail_next: AtomicBool,
        pub tables: Mutex<Vec<String>>,
        pub row_counts: Mutex<std::collections::HashMap<String, i64>>,
        pub rows: Mutex<Vec<serde_json::Value>>,
        pub columns: Mutex<Vec<(String, String, String, Option<String>)>>,
        pub primary_key: Mutex<Vec<String>>,
        pub dtako_dates: Mutex<Vec<(String, String, i64)>>,
        pub old_dates: Mutex<Vec<(String, String, i64)>>,
        pub deleted_count: Mutex<u64>,
        pub upserted: Mutex<Vec<String>>,
    }

    impl Default for MockArchiveDb {
        fn default() -> Self {
            Self {
                fail_next: AtomicBool::new(false),
                tables: Mutex::new(vec![]),
                row_counts: Mutex::new(std::collections::HashMap::new()),
                rows: Mutex::new(vec![]),
                columns: Mutex::new(vec![]),
                primary_key: Mutex::new(vec![]),
                dtako_dates: Mutex::new(vec![]),
                old_dates: Mutex::new(vec![]),
                deleted_count: Mutex::new(0),
                upserted: Mutex::new(vec![]),
            }
        }
    }

    impl MockArchiveDb {
        fn check_fail(&self) -> anyhow::Result<()> {
            if self.fail_next.swap(false, Ordering::SeqCst) {
                anyhow::bail!("mock db error");
            }
            Ok(())
        }
    }

    #[async_trait]
    impl ArchiveDb for MockArchiveDb {
        async fn fetch_columns(
            &self,
            _schema: &str,
            _table: &str,
        ) -> anyhow::Result<Vec<(String, String, String, Option<String>)>> {
            self.check_fail()?;
            Ok(self.columns.lock().unwrap().clone())
        }

        async fn fetch_primary_key(
            &self,
            _schema: &str,
            _table: &str,
        ) -> anyhow::Result<Vec<String>> {
            self.check_fail()?;
            Ok(self.primary_key.lock().unwrap().clone())
        }

        async fn list_tables(&self, _schema: &str) -> anyhow::Result<Vec<String>> {
            self.check_fail()?;
            Ok(self.tables.lock().unwrap().clone())
        }

        async fn count_rows(&self, _schema: &str, table: &str) -> anyhow::Result<i64> {
            self.check_fail()?;
            Ok(*self.row_counts.lock().unwrap().get(table).unwrap_or(&0))
        }

        async fn fetch_rows_json(
            &self,
            _schema: &str,
            _table: &str,
            limit: i64,
            offset: i64,
        ) -> anyhow::Result<Vec<serde_json::Value>> {
            self.check_fail()?;
            let rows = self.rows.lock().unwrap();
            let start = offset as usize;
            if start >= rows.len() {
                return Ok(vec![]);
            }
            let end = std::cmp::min(start + limit as usize, rows.len());
            Ok(rows[start..end].to_vec())
        }

        async fn list_dtako_dates(&self) -> anyhow::Result<Vec<(String, String, i64)>> {
            self.check_fail()?;
            Ok(self.dtako_dates.lock().unwrap().clone())
        }

        async fn fetch_dtako_rows_json(
            &self,
            _tenant_id: &str,
            _date: &str,
            limit: i64,
            offset: i64,
        ) -> anyhow::Result<Vec<serde_json::Value>> {
            self.check_fail()?;
            let rows = self.rows.lock().unwrap();
            let start = offset as usize;
            if start >= rows.len() {
                return Ok(vec![]);
            }
            let end = std::cmp::min(start + limit as usize, rows.len());
            Ok(rows[start..end].to_vec())
        }

        async fn list_old_dtako_dates(
            &self,
            _cutoff: &str,
        ) -> anyhow::Result<Vec<(String, String, i64)>> {
            self.check_fail()?;
            Ok(self.old_dates.lock().unwrap().clone())
        }

        async fn delete_dtako_date(&self, _tenant_id: &str, _date: &str) -> anyhow::Result<u64> {
            self.check_fail()?;
            Ok(*self.deleted_count.lock().unwrap())
        }

        async fn upsert_dtako_batch(&self, rows_json: &[String]) -> anyhow::Result<()> {
            self.check_fail()?;
            self.upserted
                .lock()
                .unwrap()
                .extend(rows_json.iter().cloned());
            Ok(())
        }
    }
}

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
            "SELECT * FROM alc_api.archive_list_dtako_dates()",
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
            "SELECT row_json FROM alc_api.archive_fetch_dtako_rows_json($1, $2, $3, $4)",
        )
        .bind(tenant_id)
        .bind(date)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(v,)| v).collect())
    }

    async fn upsert_dtako_batch(&self, rows_json: &[String]) -> anyhow::Result<()> {
        for row_json in rows_json {
            let v: serde_json::Value = serde_json::from_str(row_json)?;
            let arr = serde_json::Value::Array(vec![v]);
            sqlx::query("SELECT alc_api.archive_upsert_dtako_batch($1::JSONB)")
                .bind(arr)
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

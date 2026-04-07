use crate::archive::repo::ArchiveDb;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SchemaMetadata {
    pub version: u32,
    pub created_at: String,
    pub table_name: String,
    pub schema_name: String,
    pub primary_key: Vec<String>,
    pub columns: Vec<ColumnInfo>,
    pub migration_files: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub is_nullable: bool,
    pub column_default: Option<String>,
}

pub fn build_metadata(
    columns_raw: Vec<(String, String, String, Option<String>)>,
    primary_key: Vec<String>,
    schema: &str,
    table: &str,
    version: u32,
    migration_files: Vec<String>,
) -> SchemaMetadata {
    let columns: Vec<ColumnInfo> = columns_raw
        .into_iter()
        .map(|(name, data_type, nullable, default)| ColumnInfo {
            name,
            data_type,
            is_nullable: nullable == "YES",
            column_default: default,
        })
        .collect();

    SchemaMetadata {
        version,
        created_at: chrono::Utc::now().to_rfc3339(),
        table_name: table.to_string(),
        schema_name: schema.to_string(),
        primary_key,
        columns,
        migration_files,
    }
}

pub async fn fetch_schema_metadata(
    db: &dyn ArchiveDb,
    schema: &str,
    table: &str,
    version: u32,
    migration_files: Vec<String>,
) -> anyhow::Result<SchemaMetadata> {
    let columns_raw = db.fetch_columns(schema, table).await?;
    let primary_key = db.fetch_primary_key(schema, table).await?;
    Ok(build_metadata(
        columns_raw,
        primary_key,
        schema,
        table,
        version,
        migration_files,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_metadata_basic() {
        let cols = vec![
            ("id".into(), "uuid".into(), "NO".into(), None),
            (
                "name".into(),
                "text".into(),
                "YES".into(),
                Some("''".into()),
            ),
        ];
        let pk = vec!["id".into()];
        let meta = build_metadata(cols, pk, "alc_api", "dtakologs", 1, vec!["066.sql".into()]);

        assert_eq!(meta.version, 1);
        assert_eq!(meta.schema_name, "alc_api");
        assert_eq!(meta.table_name, "dtakologs");
        assert_eq!(meta.primary_key, vec!["id"]);
        assert_eq!(meta.columns.len(), 2);
        assert!(!meta.columns[0].is_nullable);
        assert!(meta.columns[1].is_nullable);
        assert_eq!(meta.columns[1].column_default.as_deref(), Some("''"));
        assert_eq!(meta.migration_files, vec!["066.sql"]);
        assert!(!meta.created_at.is_empty());
    }

    #[test]
    fn test_build_metadata_empty() {
        let meta = build_metadata(vec![], vec![], "s", "t", 2, vec![]);
        assert_eq!(meta.columns.len(), 0);
        assert_eq!(meta.primary_key.len(), 0);
        assert_eq!(meta.version, 2);
    }

    #[tokio::test]
    async fn test_fetch_schema_metadata_with_mock() {
        use crate::archive::repo::mock::MockArchiveDb;
        let db = MockArchiveDb::default();
        *db.columns.lock().unwrap() = vec![
            ("col_a".into(), "TEXT".into(), "NO".into(), None),
            (
                "col_b".into(),
                "INTEGER".into(),
                "YES".into(),
                Some("0".into()),
            ),
        ];
        *db.primary_key.lock().unwrap() = vec!["col_a".into()];

        let meta = fetch_schema_metadata(&db, "test_schema", "test_table", 1, vec![])
            .await
            .unwrap();
        assert_eq!(meta.columns.len(), 2);
        assert_eq!(meta.primary_key, vec!["col_a"]);
        assert_eq!(meta.schema_name, "test_schema");
    }

    #[tokio::test]
    async fn test_fetch_schema_metadata_db_error() {
        use crate::archive::repo::mock::MockArchiveDb;
        use std::sync::atomic::Ordering;
        let db = MockArchiveDb::default();
        db.fail_next.store(true, Ordering::SeqCst);

        let result = fetch_schema_metadata(&db, "s", "t", 1, vec![]).await;
        assert!(result.is_err());
    }
}

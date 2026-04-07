use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(Debug, Serialize, Deserialize)]
pub struct SchemaMetadata {
    pub version: u32,
    pub created_at: String,
    pub table_name: String,
    pub schema_name: String,
    pub primary_key: Vec<String>,
    pub columns: Vec<ColumnInfo>,
    pub migration_files: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub is_nullable: bool,
    pub column_default: Option<String>,
}

pub async fn fetch_schema_metadata(
    pool: &PgPool,
    schema: &str,
    table: &str,
    version: u32,
    migration_files: Vec<String>,
) -> anyhow::Result<SchemaMetadata> {
    let rows = sqlx::query_as::<_, (String, String, String, Option<String>)>(
        "SELECT column_name, data_type, is_nullable, column_default
         FROM information_schema.columns
         WHERE table_schema = $1 AND table_name = $2
         ORDER BY ordinal_position",
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await?;

    let columns: Vec<ColumnInfo> = rows
        .into_iter()
        .map(|(name, data_type, nullable, default)| ColumnInfo {
            name,
            data_type,
            is_nullable: nullable == "YES",
            column_default: default,
        })
        .collect();

    // Fetch primary key columns
    let pk_rows = sqlx::query_as::<_, (String,)>(
        "SELECT a.attname
         FROM pg_index i
         JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
         WHERE i.indrelid = ($1 || '.' || $2)::regclass AND i.indisprimary
         ORDER BY array_position(i.indkey, a.attnum)",
    )
    .bind(schema)
    .bind(table)
    .fetch_all(pool)
    .await?;

    let primary_key: Vec<String> = pk_rows.into_iter().map(|(name,)| name).collect();

    Ok(SchemaMetadata {
        version,
        created_at: chrono::Utc::now().to_rfc3339(),
        table_name: table.to_string(),
        schema_name: schema.to_string(),
        primary_key,
        columns,
        migration_files,
    })
}

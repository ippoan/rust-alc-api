//! `vein_templates` (乗務員 1 人 1 件の指静脈テンプレート、migrations/151) の読み書き。
//!
//! `alc-maintenance` と同じく trait + Pg 実装を 1 ファイルに置く (handler のテストが
//! mock に差し替えられるよう `VeinState` は trait object で持つ)。RLS に加えて
//! `WHERE tenant_id` を明示する (staging は superuser 接続で RLS が効かないため)。

use chrono::{DateTime, Utc};
use serde::Serialize;
#[cfg(feature = "sqlx")]
use sqlx::PgPool;
use uuid::Uuid;

#[cfg(feature = "sqlx")]
use alc_core::tenant::TenantConn;
use alc_core_wasm::DbError;

/// 照合に使う 1 行 (乗務員名つき。削除済みの乗務員は含めない)。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct VeinTemplateRow {
    pub id: Uuid,
    pub employee_id: Uuid,
    pub name: String,
    pub template: String,
    pub updated_at: DateTime<Utc>,
}

/// `GET /vein/templates` の 1 件 (オフライン照合用)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VeinTemplateItem {
    pub employee_id: Uuid,
    pub template: String,
    pub updated_at: DateTime<Utc>,
}

impl From<VeinTemplateRow> for VeinTemplateItem {
    fn from(r: VeinTemplateRow) -> Self {
        Self {
            employee_id: r.employee_id,
            template: r.template,
            updated_at: r.updated_at,
        }
    }
}

/// `vein_templates` の SQL。sqlx の Pg 実装 (モノリス) が使う (placeholder は `$n`)。
/// Worker の実装は ippoan/alc-vein-worker に移った (そちらは自分の写しを持つ)。
/// RLS に加えて `WHERE tenant_id` を明示する。
pub mod sql {
    /// 乗務員のテンプレートを登録し直す。$1 tenant_id / $2 employee_id / $3 template → updated_at (乗務員が居なければ 0 行)。
    pub const UPSERT: &str = r#"INSERT INTO vein_templates (tenant_id, employee_id, template)
    SELECT e.tenant_id, e.id, $3 FROM employees e
    WHERE e.tenant_id = $1 AND e.id = $2 AND e.deleted_at IS NULL
    ON CONFLICT (tenant_id, employee_id)
    DO UPDATE SET template = EXCLUDED.template, updated_at = NOW()
    RETURNING updated_at"#;

    /// テナントの全テンプレート (削除済みの乗務員を除く、登録順)。$1 tenant_id → id, employee_id, name, template, updated_at。
    pub const LIST: &str = r#"SELECT v.id, v.employee_id, e.name, v.template, v.updated_at
    FROM vein_templates v
    JOIN employees e ON e.id = v.employee_id AND e.tenant_id = v.tenant_id
    WHERE v.tenant_id = $1 AND e.deleted_at IS NULL
    ORDER BY v.created_at, v.id"#;

    /// 照合に載る登録の人数と、$2 employee_id が既に居るか。$1 tenant_id → (COUNT, BOOL)。
    pub const REGISTRATION_COUNT: &str = r#"SELECT COUNT(*), COALESCE(BOOL_OR(v.employee_id = $2), FALSE)
    FROM vein_templates v
    JOIN employees e ON e.id = v.employee_id AND e.tenant_id = v.tenant_id
    WHERE v.tenant_id = $1 AND e.deleted_at IS NULL"#;

    /// 学習後のテンプレートの書き戻し ($4 = 読んだときの updated_at のままのときだけ)。$1 tenant_id / $2 id / $3 template。
    pub const UPDATE_LEARNED: &str = r#"UPDATE vein_templates SET template = $3, updated_at = NOW()
    WHERE tenant_id = $1 AND id = $2 AND updated_at = $4"#;

    /// 乗務員のテンプレートを消す。$1 tenant_id / $2 employee_id。
    pub const DELETE: &str = "DELETE FROM vein_templates WHERE tenant_id = $1 AND employee_id = $2";
}

/// wasm32 (Workers) でも `Send + Sync` を外さない: routes の axum handler と
/// `Router::with_state` が Send を要求するため。!Send を持つ実装は `SendWrapper` で包む。
#[async_trait::async_trait]
pub trait VeinTemplatesRepository: Send + Sync {
    /// 乗務員のテンプレートを登録し直す (1 人 1 件)。`Ok(None)` = そのテナントに
    /// 生きている乗務員が居ない (404 に写す)。
    async fn upsert(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        template: &str,
    ) -> Result<Option<DateTime<Utc>>, DbError>;

    /// テナントの全テンプレート (削除済みの乗務員を除く)。並びは登録順。
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<VeinTemplateRow>, DbError>;

    /// 照合に載る登録の人数 (`list` と同じ条件 = 削除済みの乗務員を除く) と、
    /// `employee_id` がその中に既に居るか (居れば PUT は上書きで人数が増えない)。
    async fn registration_count(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<(i64, bool), DbError>;

    /// 学習後のテンプレートを書き戻す。読んだときの `updated_at` のままのときだけ書き、
    /// 間に登録し直し・別の照合の書き戻しが入っていたら何もしない (`Ok(false)`)。
    async fn update_learned(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        template: &str,
        read_updated_at: DateTime<Utc>,
    ) -> Result<bool, DbError>;

    /// 乗務員のテンプレートを消す。`Ok(false)` = 登録が無い。
    async fn delete(&self, tenant_id: Uuid, employee_id: Uuid) -> Result<bool, DbError>;
}

#[cfg(feature = "sqlx")]
pub struct PgVeinTemplatesRepository {
    pool: PgPool,
}

#[cfg(feature = "sqlx")]
impl PgVeinTemplatesRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[cfg(feature = "sqlx")]
#[async_trait::async_trait]
impl VeinTemplatesRepository for PgVeinTemplatesRepository {
    async fn upsert(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        template: &str,
    ) -> Result<Option<DateTime<Utc>>, DbError> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_scalar(sql::UPSERT)
            .bind(tenant_id)
            .bind(employee_id)
            .bind(template)
            .fetch_optional(&mut *tc.conn)
            .await
            .map_err(DbError::from)
    }

    async fn list(&self, tenant_id: Uuid) -> Result<Vec<VeinTemplateRow>, DbError> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, VeinTemplateRow>(sql::LIST)
            .bind(tenant_id)
            .fetch_all(&mut *tc.conn)
            .await
            .map_err(DbError::from)
    }

    async fn registration_count(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<(i64, bool), DbError> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as(sql::REGISTRATION_COUNT)
            .bind(tenant_id)
            .bind(employee_id)
            .fetch_one(&mut *tc.conn)
            .await
            .map_err(DbError::from)
    }

    async fn update_learned(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        template: &str,
        read_updated_at: DateTime<Utc>,
    ) -> Result<bool, DbError> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query(sql::UPDATE_LEARNED)
            .bind(tenant_id)
            .bind(id)
            .bind(template)
            .bind(read_updated_at)
            .execute(&mut *tc.conn)
            .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn delete(&self, tenant_id: Uuid, employee_id: Uuid) -> Result<bool, DbError> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query(sql::DELETE)
            .bind(tenant_id)
            .bind(employee_id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(result.rows_affected() == 1)
    }
}

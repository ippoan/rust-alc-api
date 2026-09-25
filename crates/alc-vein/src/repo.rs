//! `vein_templates` (乗務員 1 人 1 件の指静脈テンプレート、migrations/151) の読み書き。
//!
//! `alc-maintenance` と同じく trait + Pg 実装を 1 ファイルに置く (handler のテストが
//! mock に差し替えられるよう `VeinState` は trait object で持つ)。RLS に加えて
//! `WHERE tenant_id` を明示する (staging は superuser 接続で RLS が効かないため)。

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::tenant::TenantConn;

/// 照合に使う 1 行 (乗務員名つき。削除済みの乗務員は含めない)。
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
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

#[async_trait]
pub trait VeinTemplatesRepository: Send + Sync {
    /// 乗務員のテンプレートを登録し直す (1 人 1 件)。`Ok(None)` = そのテナントに
    /// 生きている乗務員が居ない (404 に写す)。
    async fn upsert(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        template: &str,
    ) -> Result<Option<DateTime<Utc>>, sqlx::Error>;

    /// テナントの全テンプレート (削除済みの乗務員を除く)。並びは登録順。
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<VeinTemplateRow>, sqlx::Error>;

    /// 学習後のテンプレートを書き戻す。読んだときの `updated_at` のままのときだけ書き、
    /// 間に登録し直し・別の照合の書き戻しが入っていたら何もしない (`Ok(false)`)。
    async fn update_learned(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        template: &str,
        read_updated_at: DateTime<Utc>,
    ) -> Result<bool, sqlx::Error>;

    /// 乗務員のテンプレートを消す。`Ok(false)` = 登録が無い。
    async fn delete(&self, tenant_id: Uuid, employee_id: Uuid) -> Result<bool, sqlx::Error>;
}

pub struct PgVeinTemplatesRepository {
    pool: PgPool,
}

impl PgVeinTemplatesRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl VeinTemplatesRepository for PgVeinTemplatesRepository {
    async fn upsert(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        template: &str,
    ) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_scalar(
            r#"INSERT INTO vein_templates (tenant_id, employee_id, template)
            SELECT e.tenant_id, e.id, $3 FROM employees e
            WHERE e.tenant_id = $1 AND e.id = $2 AND e.deleted_at IS NULL
            ON CONFLICT (tenant_id, employee_id)
            DO UPDATE SET template = EXCLUDED.template, updated_at = NOW()
            RETURNING updated_at"#,
        )
        .bind(tenant_id)
        .bind(employee_id)
        .bind(template)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn list(&self, tenant_id: Uuid) -> Result<Vec<VeinTemplateRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, VeinTemplateRow>(
            r#"SELECT v.id, v.employee_id, e.name, v.template, v.updated_at
            FROM vein_templates v
            JOIN employees e ON e.id = v.employee_id AND e.tenant_id = v.tenant_id
            WHERE v.tenant_id = $1 AND e.deleted_at IS NULL
            ORDER BY v.created_at, v.id"#,
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn update_learned(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        template: &str,
        read_updated_at: DateTime<Utc>,
    ) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query(
            r#"UPDATE vein_templates SET template = $3, updated_at = NOW()
            WHERE tenant_id = $1 AND id = $2 AND updated_at = $4"#,
        )
        .bind(tenant_id)
        .bind(id)
        .bind(template)
        .bind(read_updated_at)
        .execute(&mut *tc.conn)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    async fn delete(&self, tenant_id: Uuid, employee_id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result =
            sqlx::query("DELETE FROM vein_templates WHERE tenant_id = $1 AND employee_id = $2")
                .bind(tenant_id)
                .bind(employee_id)
                .execute(&mut *tc.conn)
                .await?;
        Ok(result.rows_affected() == 1)
    }
}

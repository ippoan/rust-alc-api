use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::{
    CreateEmployee, Employee, EmployeeUpsertItem, EmployeeUpsertSkipped, EmployeeUpsertSummary,
    FaceDataEntry, UpdateEmployee, UpdateFace,
};

use alc_core::tenant::TenantConn;

pub use alc_core::repository::employees::*;

pub struct PgEmployeeRepository {
    pool: PgPool,
}

impl PgEmployeeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EmployeeRepository for PgEmployeeRepository {
    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateEmployee,
    ) -> Result<Employee, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Employee>(
            r#"
            INSERT INTO employees (tenant_id, code, nfc_id, name, role)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(&input.code)
        .bind(&input.nfc_id)
        .bind(&input.name)
        .bind(&input.role)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn list(&self, tenant_id: Uuid) -> Result<Vec<Employee>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Employee>(
            "SELECT * FROM employees WHERE tenant_id = $1 AND deleted_at IS NULL ORDER BY name",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Employee>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Employee>(
            "SELECT * FROM employees WHERE tenant_id = $1 AND id = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn get_by_nfc(
        &self,
        tenant_id: Uuid,
        nfc_id: &str,
    ) -> Result<Option<Employee>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Employee>(
            "SELECT * FROM employees WHERE tenant_id = $1 AND nfc_id = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(nfc_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn get_by_code(
        &self,
        tenant_id: Uuid,
        code: &str,
    ) -> Result<Option<Employee>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Employee>(
            "SELECT * FROM employees WHERE tenant_id = $1 AND code = $2 AND deleted_at IS NULL",
        )
        .bind(tenant_id)
        .bind(code)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateEmployee,
    ) -> Result<Option<Employee>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Employee>(
            r#"
            UPDATE employees SET name = $1, code = $2, role = COALESCE($5, role), updated_at = NOW()
            WHERE id = $3 AND tenant_id = $4 AND deleted_at IS NULL
            RETURNING *
            "#,
        )
        .bind(&input.name)
        .bind(&input.code)
        .bind(id)
        .bind(tenant_id)
        .bind(&input.role)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query(
            r#"
            UPDATE employees SET deleted_at = NOW(), updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn update_face(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateFace,
    ) -> Result<Option<Employee>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Employee>(
            r#"
            UPDATE employees SET
                face_photo_url = COALESCE($1, face_photo_url),
                face_embedding = COALESCE($2, face_embedding),
                face_embedding_at = CASE WHEN $2 IS NOT NULL THEN NOW() ELSE face_embedding_at END,
                face_model_version = CASE WHEN $2 IS NOT NULL THEN $5 ELSE face_model_version END,
                face_approval_status = CASE WHEN $2 IS NOT NULL THEN 'pending' ELSE face_approval_status END,
                updated_at = NOW()
            WHERE id = $3 AND tenant_id = $4 AND deleted_at IS NULL
            RETURNING *
            "#,
        )
        .bind(&input.face_photo_url)
        .bind(&input.face_embedding)
        .bind(id)
        .bind(tenant_id)
        .bind(&input.face_model_version)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn list_face_data(&self, tenant_id: Uuid) -> Result<Vec<FaceDataEntry>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, FaceDataEntry>(
            r#"
            SELECT id, face_embedding, face_embedding_at, face_model_version, face_approval_status
            FROM employees
            WHERE tenant_id = $1 AND deleted_at IS NULL AND face_embedding IS NOT NULL
              AND face_approval_status = 'approved'
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn update_license(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        issue_date: Option<chrono::NaiveDate>,
        expiry_date: Option<chrono::NaiveDate>,
    ) -> Result<Option<Employee>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Employee>(
            r#"
            UPDATE employees SET
                license_issue_date = COALESCE($1, license_issue_date),
                license_expiry_date = COALESCE($2, license_expiry_date),
                updated_at = NOW()
            WHERE id = $3 AND tenant_id = $4 AND deleted_at IS NULL
            RETURNING *
            "#,
        )
        .bind(issue_date)
        .bind(expiry_date)
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn update_nfc_id(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        nfc_id: &str,
    ) -> Result<Option<Employee>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Employee>(
            r#"
            UPDATE employees SET nfc_id = $1, updated_at = NOW()
            WHERE id = $2 AND tenant_id = $3
            RETURNING *
            "#,
        )
        .bind(nfc_id)
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn upsert_by_code(
        &self,
        tenant_id: Uuid,
        items: &[EmployeeUpsertItem],
    ) -> Result<EmployeeUpsertSummary, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        alc_core::tenant::set_current_tenant(&mut tx, &tenant_id.to_string()).await?;

        let mut created = 0usize;
        let mut updated = 0usize;
        let mut skipped: Vec<EmployeeUpsertSkipped> = Vec::new();

        for item in items {
            // (a) code 一致 (deleted_at は問わない。idx_employees_code は
            // deleted_at を見ない一意制約なので、削除済み行を無視すると INSERT が衝突する)。
            let by_code: Option<(Uuid,)> =
                sqlx::query_as("SELECT id FROM employees WHERE tenant_id = $1 AND code = $2")
                    .bind(tenant_id)
                    .bind(&item.code)
                    .fetch_optional(&mut *tx)
                    .await?;

            if let Some((id,)) = by_code {
                // ★ `nfc_id` は `UNIQUE (tenant_id, nfc_id)` (001_create_tables.sql)。
                // **他の乗務員が同じ nfc_id を持っていると、その 1 行のせいで
                // トランザクションごと 500 になる** (2026-09-02 本番、退職者を含めた
                // 242 件で発生。免許の交付日+期限が同じ乗務員が居ると起きる)。
                // 衝突しているときは **nfc_id だけ据え置いて残りを更新**し、
                // `skipped` に名指しで残す — 1 人のせいで全員分を落とさない。
                let conflicting: Option<(Uuid,)> = match &item.nfc_id {
                    Some(nfc_id) => {
                        sqlx::query_as(
                            "SELECT id FROM employees WHERE tenant_id = $1 AND nfc_id = $2 AND id <> $3",
                        )
                        .bind(tenant_id)
                        .bind(nfc_id)
                        .bind(id)
                        .fetch_optional(&mut *tx)
                        .await?
                    }
                    None => None,
                };

                if conflicting.is_some() {
                    sqlx::query(
                        r#"
                        UPDATE employees SET
                            name = $1,
                            license_issue_date = $2, license_expiry_date = $3,
                            deleted_at = NULL, updated_at = NOW()
                        WHERE id = $4
                        "#,
                    )
                    .bind(&item.name)
                    .bind(item.license_issue_date)
                    .bind(item.license_expiry_date)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                    updated += 1;
                    skipped.push(EmployeeUpsertSkipped {
                        code: item.code.clone(),
                        reason: "nfc_id_conflict".to_string(),
                    });
                    continue;
                }

                sqlx::query(
                    r#"
                    UPDATE employees SET
                        name = $1, nfc_id = $2,
                        license_issue_date = $3, license_expiry_date = $4,
                        deleted_at = NULL, updated_at = NOW()
                    WHERE id = $5
                    "#,
                )
                .bind(&item.name)
                .bind(&item.nfc_id)
                .bind(item.license_issue_date)
                .bind(item.license_expiry_date)
                .bind(id)
                .execute(&mut *tx)
                .await?;
                updated += 1;
                continue;
            }

            // (b) code 未登録なら nfc_id (交付日+期限16桁) で引く。
            if let Some(nfc_id) = &item.nfc_id {
                let by_nfc: Vec<(Uuid, Option<String>)> = sqlx::query_as(
                    "SELECT id, code FROM employees WHERE tenant_id = $1 AND nfc_id = $2 AND deleted_at IS NULL",
                )
                .bind(tenant_id)
                .bind(nfc_id)
                .fetch_all(&mut *tx)
                .await?;

                if by_nfc.len() == 1 {
                    let (id, existing_code) = &by_nfc[0];
                    let assignable = existing_code.is_none()
                        || existing_code.as_deref() == Some(item.code.as_str());
                    if assignable {
                        sqlx::query(
                            r#"
                            UPDATE employees SET
                                code = $1, name = $2,
                                license_issue_date = $3, license_expiry_date = $4,
                                updated_at = NOW()
                            WHERE id = $5
                            "#,
                        )
                        .bind(&item.code)
                        .bind(&item.name)
                        .bind(item.license_issue_date)
                        .bind(item.license_expiry_date)
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                        updated += 1;
                        continue;
                    }
                    skipped.push(EmployeeUpsertSkipped {
                        code: item.code.clone(),
                        reason: "nfc_id_conflict".to_string(),
                    });
                    continue;
                } else if by_nfc.len() > 1 {
                    skipped.push(EmployeeUpsertSkipped {
                        code: item.code.clone(),
                        reason: "nfc_id_conflict".to_string(),
                    });
                    continue;
                }
            }

            // (c) 新規作成。(a)(b) で code / nfc_id の衝突は見ているが、念のため
            // ON CONFLICT DO NOTHING で残る unique 違反を検出して skip する。
            let inserted: Option<(Uuid,)> = sqlx::query_as(
                r#"
                INSERT INTO employees (tenant_id, code, nfc_id, name, role, license_issue_date, license_expiry_date)
                VALUES ($1, $2, $3, $4, ARRAY['driver'], $5, $6)
                ON CONFLICT DO NOTHING
                RETURNING id
                "#,
            )
            .bind(tenant_id)
            .bind(&item.code)
            .bind(&item.nfc_id)
            .bind(&item.name)
            .bind(item.license_issue_date)
            .bind(item.license_expiry_date)
            .fetch_optional(&mut *tx)
            .await?;

            if inserted.is_some() {
                created += 1;
            } else {
                skipped.push(EmployeeUpsertSkipped {
                    code: item.code.clone(),
                    reason: "unique_violation".to_string(),
                });
            }
        }

        tx.commit().await?;
        Ok(EmployeeUpsertSummary {
            created,
            updated,
            skipped,
        })
    }

    async fn approve_face(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Employee>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Employee>(
            r#"
            UPDATE employees SET
                face_approval_status = 'approved',
                face_approved_at = NOW(),
                updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND face_approval_status = 'pending' AND deleted_at IS NULL
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn reject_face(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Employee>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Employee>(
            r#"
            UPDATE employees SET
                face_approval_status = 'rejected',
                updated_at = NOW()
            WHERE id = $1 AND tenant_id = $2 AND face_approval_status = 'pending' AND deleted_at IS NULL
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }
}

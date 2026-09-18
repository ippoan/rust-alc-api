//! 整備記録 API (`maintenance_records`)。Refs ippoan/rust-alc-api#651。
//!
//! 国交省の遠隔点呼要件 2-四-ト「運行に使用する事業用自動車の整備状況」を満たすための、
//! **車両本体**の整備記録 (定期点検・修理・部品交換等)。`vehicles.rs` と同じく
//! `RecordsRepository` trait (+ `PgRecordsRepository` 実装) の 1 ファイル構成にする。
//!
//! ★ `vehicle_id` / `category_id` は `maintenance_records` の FK が「行が存在すること」
//! しか保証せず、**テナントの一致は保証しない** (別テナントの行を指しても FK は通り、
//! そのまま INSERT すると RLS の `WITH CHECK` で弾かれるが、それは 500 として現れて
//! しまう)。そのため作成・更新時に `vehicle_belongs_to_tenant` / `category_belongs_to_tenant`
//! で明示的に確認し、無ければ 400 を返す。確認クエリにも `tenant_id` の述語を入れる
//! (RLS 任せにしない — この repo の作法)。`category_id` の検証は `categories.rs`
//! (#c651-4) の実装に依存せず、`maintenance_categories` を直接クエリする。

use async_trait::async_trait;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json, Router,
};
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::auth_middleware::TenantId;
use alc_core::tenant::TenantConn;

use crate::models::{
    CreateMaintenanceRecord, MaintenanceRecord, MaintenanceRecordListFilter,
    MaintenanceRecordsResponse, UpdateMaintenanceRecord,
};
use crate::MaintenanceState;

const DEFAULT_PER_PAGE: i64 = 20;
const MAX_PER_PAGE: i64 = 100;

/// SELECT / RETURNING で毎回並べる列リスト。`cost` は `NUMERIC(12,2)` を `::text`
/// キャストして文字列で受け取る (`alc-trouble::repo::trouble_tickets` の
/// `damage_amount::text` と同じ作法)。
const RECORD_COLUMNS: &str = r#"id, tenant_id, vehicle_id, category_id, performed_on,
    odometer_km, vendor, description, cost::text, next_due_on,
    created_by, created_at, updated_at, deleted_at"#;

/// `maintenance_records` への読み書き。テスト側は mock 実装に差し替える。
#[async_trait]
pub trait RecordsRepository: Send + Sync {
    async fn list(
        &self,
        tenant_id: Uuid,
        filter: &MaintenanceRecordListFilter,
    ) -> Result<MaintenanceRecordsResponse, sqlx::Error>;

    async fn create(
        &self,
        tenant_id: Uuid,
        created_by: Option<Uuid>,
        input: &CreateMaintenanceRecord,
    ) -> Result<MaintenanceRecord, sqlx::Error>;

    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<MaintenanceRecord>, sqlx::Error>;

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateMaintenanceRecord,
    ) -> Result<Option<MaintenanceRecord>, sqlx::Error>;

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;

    /// `vehicle_id` が自テナントの `maintenance_vehicles` に存在するか
    /// (ソフト削除済みは対象外)。作成・更新時の FK テナント検証に使う。
    async fn vehicle_belongs_to_tenant(
        &self,
        tenant_id: Uuid,
        vehicle_id: Uuid,
    ) -> Result<bool, sqlx::Error>;

    /// `category_id` が自テナントの `maintenance_categories` に存在するか。
    /// `maintenance_categories` にソフト削除の列は無い (migrations/147)。
    async fn category_belongs_to_tenant(
        &self,
        tenant_id: Uuid,
        category_id: Uuid,
    ) -> Result<bool, sqlx::Error>;
}

pub struct PgRecordsRepository {
    pool: PgPool,
}

impl PgRecordsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RecordsRepository for PgRecordsRepository {
    async fn list(
        &self,
        tenant_id: Uuid,
        filter: &MaintenanceRecordListFilter,
    ) -> Result<MaintenanceRecordsResponse, sqlx::Error> {
        let per_page = filter
            .per_page
            .unwrap_or(DEFAULT_PER_PAGE)
            .clamp(1, MAX_PER_PAGE);
        let page = filter.page.unwrap_or(1).max(1);
        let offset = (page - 1) * per_page;

        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;

        let mut where_clauses = vec![
            "tenant_id = $1".to_string(),
            "deleted_at IS NULL".to_string(),
        ];
        let mut idx = 2u32;

        if filter.vehicle_id.is_some() {
            where_clauses.push(format!("vehicle_id = ${idx}"));
            idx += 1;
        }
        if filter.category_id.is_some() {
            where_clauses.push(format!("category_id = ${idx}"));
            idx += 1;
        }
        if filter.date_from.is_some() {
            where_clauses.push(format!("performed_on >= ${idx}"));
            idx += 1;
        }
        if filter.date_to.is_some() {
            where_clauses.push(format!("performed_on <= ${idx}"));
            idx += 1;
        }
        if filter.q.is_some() {
            where_clauses.push(format!(
                "(COALESCE(description, '') ILIKE '%' || ${idx} || '%' \
                 OR COALESCE(vendor, '') ILIKE '%' || ${idx} || '%')"
            ));
            idx += 1;
        }

        let where_sql = where_clauses.join(" AND ");

        let count_sql = format!("SELECT COUNT(*) FROM maintenance_records WHERE {where_sql}");
        let list_sql = format!(
            r#"SELECT {RECORD_COLUMNS}
            FROM maintenance_records
            WHERE {where_sql}
            ORDER BY performed_on DESC, created_at DESC
            LIMIT ${idx} OFFSET ${}"#,
            idx + 1
        );

        let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql).bind(tenant_id);
        let mut list_q = sqlx::query_as::<_, MaintenanceRecord>(&list_sql).bind(tenant_id);

        macro_rules! bind_filters {
            ($q:expr) => {
                if let Some(ref v) = filter.vehicle_id {
                    $q = $q.bind(v);
                }
                if let Some(ref v) = filter.category_id {
                    $q = $q.bind(v);
                }
                if let Some(ref v) = filter.date_from {
                    $q = $q.bind(v);
                }
                if let Some(ref v) = filter.date_to {
                    $q = $q.bind(v);
                }
                if let Some(ref v) = filter.q {
                    $q = $q.bind(v);
                }
            };
        }

        bind_filters!(count_q);
        bind_filters!(list_q);
        list_q = list_q.bind(per_page).bind(offset);

        let total = count_q.fetch_one(&mut *tc.conn).await?;
        let records = list_q.fetch_all(&mut *tc.conn).await?;

        Ok(MaintenanceRecordsResponse {
            records,
            total,
            page,
            per_page,
        })
    }

    async fn create(
        &self,
        tenant_id: Uuid,
        created_by: Option<Uuid>,
        input: &CreateMaintenanceRecord,
    ) -> Result<MaintenanceRecord, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, MaintenanceRecord>(&format!(
            r#"INSERT INTO maintenance_records (
                tenant_id, vehicle_id, category_id, performed_on,
                odometer_km, vendor, description, cost, next_due_on, created_by
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING {RECORD_COLUMNS}"#
        ))
        .bind(tenant_id)
        .bind(input.vehicle_id)
        .bind(input.category_id)
        .bind(input.performed_on)
        .bind(input.odometer_km)
        .bind(&input.vendor)
        .bind(&input.description)
        .bind(input.cost)
        .bind(input.next_due_on)
        .bind(created_by)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<MaintenanceRecord>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, MaintenanceRecord>(&format!(
            r#"SELECT {RECORD_COLUMNS}
            FROM maintenance_records
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL"#
        ))
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateMaintenanceRecord,
    ) -> Result<Option<MaintenanceRecord>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, MaintenanceRecord>(&format!(
            r#"UPDATE maintenance_records SET
                vehicle_id = COALESCE($3, vehicle_id),
                category_id = COALESCE($4, category_id),
                performed_on = COALESCE($5, performed_on),
                odometer_km = COALESCE($6, odometer_km),
                vendor = COALESCE($7, vendor),
                description = COALESCE($8, description),
                cost = COALESCE($9, cost),
                next_due_on = COALESCE($10, next_due_on),
                updated_at = now()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            RETURNING {RECORD_COLUMNS}"#
        ))
        .bind(id)
        .bind(tenant_id)
        .bind(input.vehicle_id)
        .bind(input.category_id)
        .bind(input.performed_on)
        .bind(input.odometer_km)
        .bind(&input.vendor)
        .bind(&input.description)
        .bind(input.cost)
        .bind(input.next_due_on)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query(
            "UPDATE maintenance_records SET deleted_at = now(), updated_at = now() \
             WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn vehicle_belongs_to_tenant(
        &self,
        tenant_id: Uuid,
        vehicle_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        // RLS (FORCE ROW LEVEL SECURITY) だけに任せず、この repo の作法通り
        // WHERE 句にも tenant_id を明示する (`vehicles.rs` の各メソッドと揃える)。
        sqlx::query_scalar::<_, bool>(
            r#"SELECT EXISTS(
                SELECT 1 FROM maintenance_vehicles
                WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            )"#,
        )
        .bind(vehicle_id)
        .bind(tenant_id)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn category_belongs_to_tenant(
        &self,
        tenant_id: Uuid,
        category_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        // `categories.rs` (#c651-4) の実装には依存せず、表を直接クエリする
        // (表は migrations/147 で既に用意済み)。RLS 任せにせず tenant_id を明示。
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM maintenance_categories WHERE id = $1 AND tenant_id = $2)",
        )
        .bind(category_id)
        .bind(tenant_id)
        .fetch_one(&mut *tc.conn)
        .await
    }
}

pub fn tenant_router<S>() -> Router<S>
where
    MaintenanceState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/maintenance/records",
            axum::routing::get(list_records).post(create_record),
        )
        .route(
            "/maintenance/records/{id}",
            axum::routing::get(get_record)
                .put(update_record)
                .delete(delete_record),
        )
}

async fn list_records(
    State(state): State<MaintenanceState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<MaintenanceRecordListFilter>,
) -> Result<Json<MaintenanceRecordsResponse>, StatusCode> {
    let response = state
        .records
        .list(tenant.0 .0, &filter)
        .await
        .map_err(|e| {
            tracing::error!("list_records error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(response))
}

async fn create_record(
    State(state): State<MaintenanceState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateMaintenanceRecord>,
) -> Result<(StatusCode, Json<MaintenanceRecord>), StatusCode> {
    let tenant_id = tenant.0 .0;

    let vehicle_ok = state
        .records
        .vehicle_belongs_to_tenant(tenant_id, body.vehicle_id)
        .await
        .map_err(|e| {
            tracing::error!("create_record vehicle check error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if !vehicle_ok {
        return Err(StatusCode::BAD_REQUEST);
    }

    let category_ok = state
        .records
        .category_belongs_to_tenant(tenant_id, body.category_id)
        .await
        .map_err(|e| {
            tracing::error!("create_record category check error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if !category_ok {
        return Err(StatusCode::BAD_REQUEST);
    }

    // created_by は AuthUser の user_id を持たない TenantId しか無いため、他の
    // handler (`alc-trouble::tickets::create_ticket`) と同じく None のままにする。
    let record = state
        .records
        .create(tenant_id, None, &body)
        .await
        .map_err(|e| {
            tracing::error!("create_record error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok((StatusCode::CREATED, Json(record)))
}

async fn get_record(
    State(state): State<MaintenanceState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<MaintenanceRecord>, StatusCode> {
    let record = state
        .records
        .get(tenant.0 .0, id)
        .await
        .map_err(|e| {
            tracing::error!("get_record error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(record))
}

async fn update_record(
    State(state): State<MaintenanceState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateMaintenanceRecord>,
) -> Result<Json<MaintenanceRecord>, StatusCode> {
    let tenant_id = tenant.0 .0;

    if let Some(vehicle_id) = body.vehicle_id {
        let ok = state
            .records
            .vehicle_belongs_to_tenant(tenant_id, vehicle_id)
            .await
            .map_err(|e| {
                tracing::error!("update_record vehicle check error: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if !ok {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    if let Some(category_id) = body.category_id {
        let ok = state
            .records
            .category_belongs_to_tenant(tenant_id, category_id)
            .await
            .map_err(|e| {
                tracing::error!("update_record category check error: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        if !ok {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    let record = state
        .records
        .update(tenant_id, id, &body)
        .await
        .map_err(|e| {
            tracing::error!("update_record error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(record))
}

async fn delete_record(
    State(state): State<MaintenanceState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let deleted = state
        .records
        .soft_delete(tenant.0 .0, id)
        .await
        .map_err(|e| {
            tracing::error!("delete_record error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

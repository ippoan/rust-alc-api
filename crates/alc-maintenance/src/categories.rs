//! 整備カテゴリ API (`maintenance_categories`)。Refs ippoan/rust-alc-api#651。
//!
//! `alc-core::master_data` の generic CRUD (`alc-trouble` の 4 本の後を追う
//! 5 番目の利用者、Refs #651) にテーブル名だけ差し替えて乗る。auto-seed の
//! 作法は `alc-trouble/src/categories.rs` を手本にしている。
//! trait + Pg 実装を 1 ファイルにまとめる形は `alc-maintenance/src/vehicles.rs`
//! と同じ (`alc-trouble` の `repository`/`repo` 分割はしない)。

use async_trait::async_trait;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::auth_middleware::TenantId;
use alc_core::master_data::{self, MasterCreateInput, MasterTable};
use alc_core::master_handlers::{is_conflict, UpdateSortOrder};

use crate::models::{CreateMaintenanceCategory, MaintenanceCategory};
use crate::MaintenanceState;

/// 一覧が空のテナントに自動で入れる既定カテゴリ (Refs #651)。
const DEFAULT_CATEGORIES: &[&str] = &["定期点検", "修理", "部品交換", "タイヤ交換", "オイル交換"];

impl MasterCreateInput for CreateMaintenanceCategory {
    fn name(&self) -> &str {
        &self.name
    }

    fn sort_order(&self) -> Option<i32> {
        self.sort_order
    }
}

/// `maintenance_categories` への読み書き。テスト側は mock 実装に差し替える
/// (`VehiclesRepository` と同じ形)。
#[async_trait]
pub trait MaintenanceCategoriesRepository: Send + Sync {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<MaintenanceCategory>, sqlx::Error>;

    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateMaintenanceCategory,
    ) -> Result<MaintenanceCategory, sqlx::Error>;

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;

    async fn update_sort_order(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        sort_order: i32,
    ) -> Result<Option<MaintenanceCategory>, sqlx::Error>;
}

pub struct PgMaintenanceCategoriesRepository {
    pool: PgPool,
}

/// SQL に埋め込むテーブル名はこのリテラルだけ (Refs #651 — SQL injection の境界。
/// `alc_core::master_data` のモジュールコメント参照)。
impl MasterTable for PgMaintenanceCategoriesRepository {
    const TABLE: &'static str = "maintenance_categories";
}

impl PgMaintenanceCategoriesRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MaintenanceCategoriesRepository for PgMaintenanceCategoriesRepository {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<MaintenanceCategory>, sqlx::Error> {
        master_data::list::<Self, _>(&self.pool, tenant_id).await
    }

    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateMaintenanceCategory,
    ) -> Result<MaintenanceCategory, sqlx::Error> {
        master_data::create::<Self, _, _>(&self.pool, tenant_id, input).await
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        master_data::delete::<Self>(&self.pool, tenant_id, id).await
    }

    async fn update_sort_order(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        sort_order: i32,
    ) -> Result<Option<MaintenanceCategory>, sqlx::Error> {
        master_data::update_sort_order::<Self, _>(&self.pool, tenant_id, id, sort_order).await
    }
}

pub fn tenant_router<S>() -> Router<S>
where
    MaintenanceState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/maintenance/categories",
            get(list_categories).post(create_category),
        )
        .route(
            "/maintenance/categories/{id}",
            delete(delete_category).put(update_category_sort),
        )
}

async fn list_categories(
    State(state): State<MaintenanceState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<MaintenanceCategory>>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let categories = state.categories.list(tenant_id).await.map_err(|e| {
        tracing::error!("list_categories error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if !categories.is_empty() {
        return Ok(Json(categories));
    }

    // 空のテナントには既定カテゴリを自動 seed する (alc-trouble と同じ作法)。
    let mut seeded = Vec::new();
    for (i, name) in DEFAULT_CATEGORIES.iter().enumerate() {
        let input = CreateMaintenanceCategory {
            name: name.to_string(),
            sort_order: Some(i as i32 + 1),
        };
        match state.categories.create(tenant_id, &input).await {
            Ok(cat) => seeded.push(cat),
            Err(e) => {
                tracing::warn!("auto-seed maintenance category {name}: {e}");
            }
        }
    }
    Ok(Json(seeded))
}

async fn create_category(
    State(state): State<MaintenanceState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateMaintenanceCategory>,
) -> Result<(StatusCode, Json<MaintenanceCategory>), StatusCode> {
    if body.name.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let category = state
        .categories
        .create(tenant.0 .0, &body)
        .await
        .map_err(|e| {
            if is_conflict(&e) {
                return StatusCode::CONFLICT;
            }
            tracing::error!("create_category error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok((StatusCode::CREATED, Json(category)))
}

async fn update_category_sort(
    State(state): State<MaintenanceState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateSortOrder>,
) -> Result<Json<MaintenanceCategory>, StatusCode> {
    let category = state
        .categories
        .update_sort_order(tenant.0 .0, id, body.sort_order)
        .await
        .map_err(|e| {
            tracing::error!("update_category_sort error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(category))
}

async fn delete_category(
    State(state): State<MaintenanceState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let deleted = state
        .categories
        .delete(tenant.0 .0, id)
        .await
        .map_err(|e| {
            tracing::error!("delete_category error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

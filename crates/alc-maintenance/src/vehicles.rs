//! 車両マスタ API (`maintenance_vehicles`)。Refs ippoan/rust-alc-api#651。
//!
//! 車両 identity の正本はこのテーブル。電子車検証 (carins) は従属する任意のリンクで、
//! `registration_number` だけで登録できる (`car_id` は後から `PUT .../carins` で紐づける)。
//!
//! `alc-trouble` の `repository` (trait) + `repo` (Pg 実装) の分割を 1 ファイルに
//! まとめた形 (`VehiclesRepository` trait → `PgVehiclesRepository` — テスト側が
//! mock 実装に差し替えられるようにするため trait object で持つ、Refs #513 Phase B
//! と同じ理由)。

use async_trait::async_trait;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json, Router,
};
use sqlx::{Acquire, PgPool};
use uuid::Uuid;

use alc_core::auth_middleware::TenantId;
use alc_core::repository::car_inspections::normalize_carins_numbers;
use alc_core::tenant::TenantConn;

use crate::models::{
    CarinsCandidate, CarinsImportCandidate, CarinsImportRequest, CarinsImportResult,
    CreateMaintenanceVehicle, LinkCarinsRequest, MaintenanceVehicle, UpdateMaintenanceVehicle,
    VehicleListFilter, VehicleListResponse,
};
use crate::MaintenanceState;

const DEFAULT_PER_PAGE: i64 = 20;
const MAX_PER_PAGE: i64 = 100;
/// `POST .../carins-import` の 1 リクエストあたりの上限。1 トランザクションで回すため
/// 際限なく受けない (候補一覧は通常テナントあたり数十件、Refs #662)。
const MAX_IMPORT_CAR_IDS: usize = 500;

/// 全角の数字・ダッシュを半角へ正規化する。SQL 側 (`crates/alc-trouble/src/repo/
/// trouble_tickets.rs:153` の一覧検索 `translate(...)`) と同じ文字集合を Rust 側で
/// 1 回だけ適用する (nuxt-trouble はこれをクライアント側でやっているが、こちらは
/// server 側で 1 回)。
pub fn normalize_registration_number(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '０' => '0',
            '１' => '1',
            '２' => '2',
            '３' => '3',
            '４' => '4',
            '５' => '5',
            '６' => '6',
            '７' => '7',
            '８' => '8',
            '９' => '9',
            '－' | 'ー' => '-',
            other => other,
        })
        .collect()
}

/// `maintenance_vehicles` への読み書き。テスト側は mock 実装に差し替える。
#[async_trait]
pub trait VehiclesRepository: Send + Sync {
    async fn list(
        &self,
        tenant_id: Uuid,
        filter: &VehicleListFilter,
    ) -> Result<VehicleListResponse, sqlx::Error>;

    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateMaintenanceVehicle,
    ) -> Result<MaintenanceVehicle, sqlx::Error>;

    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<MaintenanceVehicle>, sqlx::Error>;

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateMaintenanceVehicle,
    ) -> Result<Option<MaintenanceVehicle>, sqlx::Error>;

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;

    /// `car_id` を紐づける。`Ok(None)` = 対象車両が無い (404 に写す)。unique 違反
    /// (他の車両が既にその `car_id` を持つ) は `sqlx::Error::Database` をそのまま
    /// 返し、handler が `23505` を見て 409 に写す。
    async fn link_carins(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        car_id: &str,
    ) -> Result<Option<MaintenanceVehicle>, sqlx::Error>;

    async fn unlink_carins(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;

    /// `ElectCertMgNo` (管理番号) から一致する行の `CarId` を引く。`link_carins`
    /// ハンドラが `cert_no` だけを受けたとき (`car_id` 未指定) に、保存すべき
    /// `CarId` を得るための補助クエリ。`lookup_expiry` 自体は期限照合のみで
    /// `CarId` を返さないため必要。
    async fn fetch_car_id_by_cert_no(
        &self,
        tenant_id: Uuid,
        cert_no: &str,
    ) -> Result<Option<String>, sqlx::Error>;

    /// 正規化済みの登録番号で car_inspection の候補を引く。同一 `CarId` は最新の
    /// 交付日の 1 行に畳む。
    async fn carins_candidates(
        &self,
        tenant_id: Uuid,
        normalized_registration_number: &str,
    ) -> Result<Vec<CarinsCandidate>, sqlx::Error>;

    /// まだ `maintenance_vehicles` に取り込まれていない carins を全件返す
    /// (`carins_candidates` の反転、Refs #662)。同一 `CarId` は最新の交付日の
    /// 1 行に畳む。
    async fn carins_import_candidates(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<CarinsImportCandidate>, sqlx::Error>;

    /// 選ばれた `car_id` を **1 トランザクションで**取り込む (Refs #662)。
    /// 登録番号一致の既存車両が在れば紐づけ、無ければ作成、既に紐づけ済みなら skip。
    async fn carins_import(
        &self,
        tenant_id: Uuid,
        car_ids: &[String],
    ) -> Result<CarinsImportResult, sqlx::Error>;
}

pub fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("23505"))
}

pub struct PgVehiclesRepository {
    pool: PgPool,
}

impl PgVehiclesRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl VehiclesRepository for PgVehiclesRepository {
    async fn list(
        &self,
        tenant_id: Uuid,
        filter: &VehicleListFilter,
    ) -> Result<VehicleListResponse, sqlx::Error> {
        let per_page = filter
            .per_page
            .unwrap_or(DEFAULT_PER_PAGE)
            .clamp(1, MAX_PER_PAGE);
        let page = filter.page.unwrap_or(1).max(1);
        let offset = (page - 1) * per_page;

        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;

        let total: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM maintenance_vehicles
            WHERE tenant_id = $1 AND deleted_at IS NULL
              AND ($2::text IS NULL OR registration_number ILIKE '%' || $2 || '%'
                   OR COALESCE(display_name, '') ILIKE '%' || $2 || '%')
              AND ($3::bool IS NULL OR (car_id IS NOT NULL) = $3)"#,
        )
        .bind(tenant_id)
        .bind(&filter.q)
        .bind(filter.linked)
        .fetch_one(&mut *tc.conn)
        .await?;

        let items = sqlx::query_as::<_, MaintenanceVehicle>(
            r#"SELECT * FROM maintenance_vehicles
            WHERE tenant_id = $1 AND deleted_at IS NULL
              AND ($2::text IS NULL OR registration_number ILIKE '%' || $2 || '%'
                   OR COALESCE(display_name, '') ILIKE '%' || $2 || '%')
              AND ($3::bool IS NULL OR (car_id IS NOT NULL) = $3)
            ORDER BY created_at DESC
            LIMIT $4 OFFSET $5"#,
        )
        .bind(tenant_id)
        .bind(&filter.q)
        .bind(filter.linked)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&mut *tc.conn)
        .await?;

        Ok(VehicleListResponse {
            items,
            total,
            page,
            per_page,
        })
    }

    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateMaintenanceVehicle,
    ) -> Result<MaintenanceVehicle, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, MaintenanceVehicle>(
            r#"INSERT INTO maintenance_vehicles (tenant_id, registration_number, display_name, car_id, note)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *"#,
        )
        .bind(tenant_id)
        .bind(&input.registration_number)
        .bind(&input.display_name)
        .bind(&input.car_id)
        .bind(&input.note)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn get(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<MaintenanceVehicle>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, MaintenanceVehicle>(
            "SELECT * FROM maintenance_vehicles WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateMaintenanceVehicle,
    ) -> Result<Option<MaintenanceVehicle>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, MaintenanceVehicle>(
            r#"UPDATE maintenance_vehicles
            SET registration_number = COALESCE($3, registration_number),
                display_name = COALESCE($4, display_name),
                note = COALESCE($5, note),
                updated_at = now()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            RETURNING *"#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(&input.registration_number)
        .bind(&input.display_name)
        .bind(&input.note)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query(
            "UPDATE maintenance_vehicles SET deleted_at = now(), updated_at = now() \
             WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn link_carins(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        car_id: &str,
    ) -> Result<Option<MaintenanceVehicle>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, MaintenanceVehicle>(
            r#"UPDATE maintenance_vehicles
            SET car_id = $3, carins_linked_at = now(), updated_at = now()
            WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
            RETURNING *"#,
        )
        .bind(id)
        .bind(tenant_id)
        .bind(car_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn unlink_carins(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query(
            "UPDATE maintenance_vehicles SET car_id = NULL, carins_linked_at = NULL, updated_at = now() \
             WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
        )
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn fetch_car_id_by_cert_no(
        &self,
        tenant_id: Uuid,
        cert_no: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        // RLS (FORCE ROW LEVEL SECURITY) だけに任せず、この repo の作法通り
        // WHERE 句にも tenant_id を明示する (多重防御。`get`/`update`/
        // `unlink_carins` と揃える)。
        let row: Option<Option<String>> = sqlx::query_scalar(
            r#"SELECT NULLIF("CarId", '') FROM car_inspection
            WHERE "ElectCertMgNo" = $1 AND tenant_id = $2
            ORDER BY "GrantdateY" DESC, "GrantdateM" DESC, "GrantdateD" DESC LIMIT 1"#,
        )
        .bind(cert_no)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await?;
        Ok(row.flatten())
    }

    async fn carins_candidates(
        &self,
        tenant_id: Uuid,
        normalized_registration_number: &str,
    ) -> Result<Vec<CarinsCandidate>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        // EntryNoCarNo / CarNo は登録番号の一部 (末尾の分類番号等) だけを持つことが
        // 多いため、「車両の (正規化済み) 登録番号が候補の値を含むか」を見る —
        // 逆方向 (候補が登録番号全体を含むか) だと空文字列以外ほぼ一致しない。
        // 空文字列の候補は translate 後も '' のままで `'%' || '' || '%'` = `%` と
        // なり全件ヒットしてしまうため、非空のものだけを対象にする。
        //
        // 同一 CarId は最新の交付日の 1 行に畳む (`alc-carins` の
        // `repo/car_inspections.rs:34` の DISTINCT ON が手本)。
        // RLS (FORCE ROW LEVEL SECURITY) だけに任せず、この repo の作法通り
        // WHERE 句にも tenant_id を明示する (多重防御。`get`/`update`/
        // `unlink_carins`/`fetch_car_id_by_cert_no` と揃える)。
        sqlx::query_as::<_, CarinsCandidate>(
            r#"SELECT DISTINCT ON (ci."CarId")
                ci."CarId" AS car_id,
                ci."ElectCertMgNo" AS cert_no,
                COALESCE(NULLIF(ci."EntryNoCarNo", ''), ci."CarNo") AS car_no
            FROM car_inspection ci
            WHERE ci.tenant_id = $2
              AND (
                (ci."EntryNoCarNo" <> '' AND $1 ILIKE '%' || translate(ci."EntryNoCarNo", '０１２３４５６７８９－ー', '0123456789--') || '%')
                OR (ci."CarNo" <> '' AND $1 ILIKE '%' || translate(ci."CarNo", '０１２３４５６７８９－ー', '0123456789--') || '%')
              )
            ORDER BY ci."CarId", ci."GrantdateY" DESC, ci."GrantdateM" DESC, ci."GrantdateD" DESC"#,
        )
        .bind(normalized_registration_number)
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn carins_import_candidates(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<CarinsImportCandidate>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        // `carins_candidates` の反転 — あちらは「この車両の登録番号に一致する候補」、
        // こちらは「まだ取り込まれていない carins 全部」(Refs #662)。
        //
        // 同一 CarId は最新の交付日の 1 行に畳む (`car_inspection` は車検証 1 枚ごとの
        // 履歴表。`alc-carins` の `repo/car_inspections.rs:34` の DISTINCT ON が手本)。
        // 突き合わせは SQL 側の translate で行う — Rust 側の
        // `normalize_registration_number` は単体の候補検索用で、N 件を SQL 内で畳む
        // こちらでは使わない。
        //
        // RLS (FORCE ROW LEVEL SECURITY) だけに任せず、この repo の作法通り
        // WHERE 句にも tenant_id を明示する (多重防御)。JOIN する
        // `maintenance_vehicles` 側にも同じく明示して片肺にしない。
        sqlx::query_as::<_, CarinsImportCandidate>(
            r#"SELECT DISTINCT ON (ci."CarId")
                ci."CarId" AS car_id,
                ci."ElectCertMgNo" AS cert_no,
                COALESCE(NULLIF(ci."EntryNoCarNo", ''), ci."CarNo") AS car_no,
                mv.id AS existing_vehicle_id
            FROM car_inspection ci
            LEFT JOIN maintenance_vehicles mv
              ON mv.tenant_id = $1
             AND mv.deleted_at IS NULL
             AND translate(mv.registration_number, '０１２３４５６７８９－ー', '0123456789--')
                 = translate(COALESCE(NULLIF(ci."EntryNoCarNo", ''), ci."CarNo"), '０１２３４５６７８９－ー', '0123456789--')
            WHERE ci.tenant_id = $1
              AND ci."CarId" <> ''
              AND COALESCE(NULLIF(ci."EntryNoCarNo", ''), ci."CarNo") <> ''
              AND NOT EXISTS (
                SELECT 1 FROM maintenance_vehicles linked
                WHERE linked.tenant_id = $1 AND linked.car_id = ci."CarId"
              )
            ORDER BY ci."CarId", ci."GrantdateY" DESC, ci."GrantdateM" DESC, ci."GrantdateD" DESC,
                     mv.created_at"#,
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn carins_import(
        &self,
        tenant_id: Uuid,
        car_ids: &[String],
    ) -> Result<CarinsImportResult, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        // 「POST で作成 → PUT .../carins で紐づけ」をフロントに 2 コールさせると、
        // 間で失敗したとき裸の車両行が残る。`registration_number` の index は
        // UNIQUE ではない (migrations/147:34。UNIQUE は :30 の
        // `(tenant_id, car_id) WHERE car_id IS NOT NULL` だけ) ため再試行のたびに
        // 重複行が増えるので、1 トランザクションに寄せる (Refs #662)。
        let mut tx = tc.conn.begin().await?;
        let mut result = CarinsImportResult {
            created: 0,
            linked: 0,
            skipped: 0,
        };

        for car_id in car_ids {
            let car_id = car_id.trim();
            if car_id.is_empty() {
                result.skipped += 1;
                continue;
            }

            // (1) 既にこの car_id を持つ車両が在れば skip (冪等)。deleted_at は見ない
            // — partial unique (migrations/147:30) も見ないので、soft delete 済みの
            // 行が持つ car_id は依然として後続の INSERT/UPDATE を弾く。
            let already: Option<Uuid> = sqlx::query_scalar(
                "SELECT id FROM maintenance_vehicles WHERE tenant_id = $1 AND car_id = $2 LIMIT 1",
            )
            .bind(tenant_id)
            .bind(car_id)
            .fetch_optional(&mut *tx)
            .await?;
            if already.is_some() {
                result.skipped += 1;
                continue;
            }

            // (2) 最新 1 枚の車検証から登録番号相当を引く。このテナントに無い car_id
            // (他テナントの値を投げられた場合を含む) はここで skip になる。
            let car_no: Option<String> = sqlx::query_scalar(
                r#"SELECT COALESCE(NULLIF(ci."EntryNoCarNo", ''), ci."CarNo")
                FROM car_inspection ci
                WHERE ci.tenant_id = $1 AND ci."CarId" = $2
                ORDER BY ci."GrantdateY" DESC, ci."GrantdateM" DESC, ci."GrantdateD" DESC
                LIMIT 1"#,
            )
            .bind(tenant_id)
            .bind(car_id)
            .fetch_optional(&mut *tx)
            .await?;
            let Some(car_no) = car_no.filter(|v| !v.trim().is_empty()) else {
                result.skipped += 1;
                continue;
            };

            // (3) 正規化した登録番号で既存車両を探す。未紐づけ (car_id IS NULL) を
            // 優先し、同点は古い順 — 登録番号は移転で再割当てされるため UNIQUE では
            // なく (migrations/147:34)、複数行あり得るので順序を決定的にする。
            let existing: Option<(Uuid, Option<String>)> = sqlx::query_as(
                r#"SELECT id, car_id FROM maintenance_vehicles
                WHERE tenant_id = $1 AND deleted_at IS NULL
                  AND translate(registration_number, '０１２３４５６７８９－ー', '0123456789--')
                      = translate($2, '０１２３４５６７８９－ー', '0123456789--')
                ORDER BY (car_id IS NOT NULL), created_at
                LIMIT 1"#,
            )
            .bind(tenant_id)
            .bind(&car_no)
            .fetch_optional(&mut *tx)
            .await?;

            // UNIQUE 違反 (同時実行) は skip 扱いにするが、transaction 全体を
            // abort させないよう SAVEPOINT (sqlx の入れ子 transaction) で囲む
            // (`alc-misc::repo::measurements` の `record_as_tenko_if_marked` と同じ作法)。
            match existing {
                // 一致先が既に別の車検証に紐づいている → 上書きせず skip
                Some((_, Some(_))) => {
                    result.skipped += 1;
                }
                Some((id, None)) => {
                    let mut sp = tx.begin().await?;
                    let updated = sqlx::query(
                        r#"UPDATE maintenance_vehicles
                        SET car_id = $3, carins_linked_at = now(), updated_at = now()
                        WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL"#,
                    )
                    .bind(id)
                    .bind(tenant_id)
                    .bind(car_id)
                    .execute(&mut *sp)
                    .await;
                    match updated {
                        Ok(r) if r.rows_affected() > 0 => {
                            sp.commit().await?;
                            result.linked += 1;
                        }
                        Ok(_) => {
                            sp.rollback().await?;
                            result.skipped += 1;
                        }
                        Err(e) if is_unique_violation(&e) => {
                            sp.rollback().await?;
                            result.skipped += 1;
                        }
                        Err(e) => return Err(e),
                    }
                }
                None => {
                    let mut sp = tx.begin().await?;
                    let inserted = sqlx::query(
                        r#"INSERT INTO maintenance_vehicles
                            (tenant_id, registration_number, car_id, carins_linked_at)
                        VALUES ($1, $2, $3, now())"#,
                    )
                    .bind(tenant_id)
                    .bind(&car_no)
                    .bind(car_id)
                    .execute(&mut *sp)
                    .await;
                    match inserted {
                        Ok(_) => {
                            sp.commit().await?;
                            result.created += 1;
                        }
                        Err(e) if is_unique_violation(&e) => {
                            sp.rollback().await?;
                            result.skipped += 1;
                        }
                        Err(e) => return Err(e),
                    }
                }
            }
        }

        tx.commit().await?;
        Ok(result)
    }
}

pub fn tenant_router<S>() -> Router<S>
where
    MaintenanceState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/maintenance/vehicles",
            axum::routing::get(list_vehicles).post(create_vehicle),
        )
        // 静的セグメントは `/{id}` (Path<Uuid>) より **前** に並べる
        // (`alc-carins` の `car_inspections.rs:30-32` と同じ作法。matchit は静的を
        // 優先するので衝突はしないが、読み手が順序で判断できるようにしておく)。
        .route(
            "/maintenance/vehicles/carins-import-candidates",
            axum::routing::get(carins_import_candidates),
        )
        .route(
            "/maintenance/vehicles/carins-import",
            axum::routing::post(carins_import),
        )
        .route(
            "/maintenance/vehicles/{id}",
            axum::routing::get(get_vehicle)
                .put(update_vehicle)
                .delete(delete_vehicle),
        )
        .route(
            "/maintenance/vehicles/{id}/carins",
            axum::routing::put(link_carins).delete(unlink_carins),
        )
        .route(
            "/maintenance/vehicles/{id}/carins-candidates",
            axum::routing::get(carins_candidates),
        )
}

async fn list_vehicles(
    State(state): State<MaintenanceState>,
    tenant: axum::Extension<TenantId>,
    Query(filter): Query<VehicleListFilter>,
) -> Result<Json<VehicleListResponse>, StatusCode> {
    let response = state
        .vehicles
        .list(tenant.0 .0, &filter)
        .await
        .map_err(|e| {
            tracing::error!("list_vehicles error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(response))
}

async fn create_vehicle(
    State(state): State<MaintenanceState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateMaintenanceVehicle>,
) -> Result<(StatusCode, Json<MaintenanceVehicle>), StatusCode> {
    if body.registration_number.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let vehicle = state
        .vehicles
        .create(tenant.0 .0, &body)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                return StatusCode::CONFLICT;
            }
            tracing::error!("create_vehicle error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok((StatusCode::CREATED, Json(vehicle)))
}

async fn get_vehicle(
    State(state): State<MaintenanceState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<MaintenanceVehicle>, StatusCode> {
    let vehicle = state
        .vehicles
        .get(tenant.0 .0, id)
        .await
        .map_err(|e| {
            tracing::error!("get_vehicle error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(vehicle))
}

async fn update_vehicle(
    State(state): State<MaintenanceState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateMaintenanceVehicle>,
) -> Result<Json<MaintenanceVehicle>, StatusCode> {
    if body
        .registration_number
        .as_deref()
        .is_some_and(|v| v.trim().is_empty())
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let vehicle = state
        .vehicles
        .update(tenant.0 .0, id, &body)
        .await
        .map_err(|e| {
            tracing::error!("update_vehicle error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(vehicle))
}

async fn delete_vehicle(
    State(state): State<MaintenanceState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let deleted = state
        .vehicles
        .soft_delete(tenant.0 .0, id)
        .await
        .map_err(|e| {
            tracing::error!("delete_vehicle error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn link_carins(
    State(state): State<MaintenanceState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
    Json(body): Json<LinkCarinsRequest>,
) -> Result<Json<MaintenanceVehicle>, StatusCode> {
    let tenant_id = tenant.0 .0;

    let mut cert_no = body.cert_no.clone();
    let mut car_id = body.car_id.clone();
    normalize_carins_numbers(&mut cert_no, &mut car_id).map_err(|_| StatusCode::BAD_REQUEST)?;

    let lookup = state
        .car_inspections
        .lookup_expiry(tenant_id, cert_no.as_deref(), car_id.as_deref())
        .await
        .map_err(|e| {
            tracing::error!("link_carins lookup_expiry error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if lookup.matched_by == "none" {
        return Err(StatusCode::BAD_REQUEST);
    }

    // car_id が入力で分かっていればそれを使う。cert_no だけの入力 (matched_by ==
    // "cert_no") のときは、一致した行の CarId を別途引いて保存する
    // (lookup_expiry は期限照合のみで CarId 自体は返さないため)。
    let resolved_car_id = match &car_id {
        Some(cid) => Some(cid.clone()),
        None => {
            let cn = cert_no.as_deref().ok_or(StatusCode::BAD_REQUEST)?;
            state
                .vehicles
                .fetch_car_id_by_cert_no(tenant_id, cn)
                .await
                .map_err(|e| {
                    tracing::error!("link_carins fetch_car_id_by_cert_no error: {e}");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?
        }
    };
    let resolved_car_id = resolved_car_id.ok_or(StatusCode::BAD_REQUEST)?;

    let vehicle = state
        .vehicles
        .link_carins(tenant_id, id, &resolved_car_id)
        .await
        .map_err(|e| {
            if is_unique_violation(&e) {
                return StatusCode::CONFLICT;
            }
            tracing::error!("link_carins update error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(vehicle))
}

async fn unlink_carins(
    State(state): State<MaintenanceState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let unlinked = state
        .vehicles
        .unlink_carins(tenant.0 .0, id)
        .await
        .map_err(|e| {
            tracing::error!("unlink_carins error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if unlinked {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn carins_candidates(
    State(state): State<MaintenanceState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<CarinsCandidate>>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let vehicle = state
        .vehicles
        .get(tenant_id, id)
        .await
        .map_err(|e| {
            tracing::error!("carins_candidates get vehicle error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let normalized = normalize_registration_number(&vehicle.registration_number);
    if normalized.trim().is_empty() {
        return Ok(Json(Vec::new()));
    }

    let candidates = state
        .vehicles
        .carins_candidates(tenant_id, &normalized)
        .await
        .map_err(|e| {
            tracing::error!("carins_candidates query error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(candidates))
}

async fn carins_import_candidates(
    State(state): State<MaintenanceState>,
    tenant: axum::Extension<TenantId>,
) -> Result<Json<Vec<CarinsImportCandidate>>, StatusCode> {
    let candidates = state
        .vehicles
        .carins_import_candidates(tenant.0 .0)
        .await
        .map_err(|e| {
            tracing::error!("carins_import_candidates error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(candidates))
}

async fn carins_import(
    State(state): State<MaintenanceState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CarinsImportRequest>,
) -> Result<Json<CarinsImportResult>, StatusCode> {
    if body.car_ids.is_empty() || body.car_ids.len() > MAX_IMPORT_CAR_IDS {
        return Err(StatusCode::BAD_REQUEST);
    }
    let result = state
        .vehicles
        .carins_import(tenant.0 .0, &body.car_ids)
        .await
        .map_err(|e| {
            tracing::error!("carins_import error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(result))
}

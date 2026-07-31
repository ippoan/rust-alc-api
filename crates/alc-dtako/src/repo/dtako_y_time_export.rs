//! `DtakoYTimeExportRepository` の Pg 実装。
//!
//! ## 期間の絞り込みは 読取日 **と** 運行日 の OR (Refs ohishi-exp/rust-ichibanboshi#205 の 38)
//!
//! この 4 つの列挙クエリは全部同じ期間条件を共有する:
//!
//! ```sql
//! (reading_date BETWEEN $from AND $to OR operation_date BETWEEN $from AND $to)
//! ```
//!
//! **`reading_date` (読取日) だけで絞ると月末の運行が構造的に落ちる。** 読取日は
//! 「デジタコのカードを読ませた日」なので、月末に走った運行ほど読まれるのが遅い。
//! 2026-06 の勤怠を畳んだとき、オンプレ基準より 142 行少ない day_summaries しか
//! 出なかった原因がこれ。名指しできた 29 件を `GET /api/operations/{unko_no}` で
//! 1 件ずつ引いた結果は **29/29 で反例ゼロ**だった:
//!
//! - 29 件とも alc に存在する (見つからない 0 件)
//! - 29 件とも読取日が窓の上端 (2026-07-02) より後 (07-03 〜 07-13)
//! - 29 件とも **運行日は 06-24 〜 07-01** = 窓の中。運行日で引けば全部拾える
//!
//! 対照として乗務員 1021 の最終運行 (`2606190748000000004286`、運行日 06-19 /
//! 読取日 07-01) は読取日が窓の中なので欠落ゼロ。乗務員 1078 の
//! `2606241140060000002302` (運行日 06-24 / 読取日 07-06) は `return_at` も走行距離も
//! イベント 104 件も揃っていて**閉じている**のに、読取日しか見ていなかったため
//! 「GCP に無い」ように見えていた。
//!
//! **`reading_date` 側の条件は消していない。** `operation_date` は NULL 可
//! (`migrations/054_dtako_tables.sql`) で、KUDGURI の `運行日` 列が無い/空の取り込みでは
//! 埋まらない。片方だけにすると別の取りこぼしが出るので、置き換えではなく **OR で追加**する。
//!
//! 同じ形は `repo/dtako_upload.rs` の `fetch_operations_for_recalc` /
//! `load_driver_operations` が既に採っている。上下 1 日の広げ方 (暦日をまたぐ運行の
//! 取りこぼし防止) は両方の列に同じく効かせる。
//!
//! ### 索引
//!
//! `idx_dtako_ops_reading_date (tenant_id, reading_date)` はあるが `operation_date` には
//! 無い (`migrations/054_dtako_tables.sql`)。OR は BitmapOr になるので operation_date 側は
//! 索引無しの走査になる。追加するかは本番の `dtako-etags ms` の `ms_drv` / `ms_ops` を
//! 見てから決める (Refs #205 の 38)。

use async_trait::async_trait;
use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::tenant::TenantConn;

pub use alc_core::repository::dtako_y_time_export::*;

pub struct PgDtakoYTimeExportRepository {
    pool: PgPool,
}

impl PgDtakoYTimeExportRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DtakoYTimeExportRepository for PgDtakoYTimeExportRepository {
    async fn lookup_driver(
        &self,
        tenant_id: Uuid,
        driver_cd: &str,
    ) -> Result<Option<(Uuid, String)>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let row: Option<(Uuid, String)> = sqlx::query_as(
            "SELECT id, name FROM alc_api.employees \
             WHERE tenant_id = $1 AND driver_cd = $2 AND deleted_at IS NULL \
             LIMIT 1",
        )
        .bind(tenant_id)
        .bind(driver_cd)
        .fetch_optional(&mut *tc.conn)
        .await?;
        Ok(row)
    }

    async fn list_operations(
        &self,
        tenant_id: Uuid,
        driver_id: Uuid,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<YTimeExportOperation>, sqlx::Error> {
        // 読取日/運行日を 1日広げて取り、暦日をまたぐ運行を取りこぼさない
        let from_widened = from - chrono::Duration::days(1);
        let to_widened = to + chrono::Duration::days(1);
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        // 同じ driver_id が同じ unko_no で crew_role 1 (運転手) と 2 (副運転手) の
        // 両方に登録されているケースがある (KUDGURI データ起因)。物理的に同じ運行な
        // ので 1 行にまとめる必要がある。crew_role=1 を優先 (なければ 2) して 1 件のみ採用。
        sqlx::query_as::<_, YTimeExportOperation>(
            "SELECT DISTINCT ON (unko_no) \
                    unko_no, crew_role, departure_at, return_at, r2_key_prefix \
             FROM alc_api.dtako_operations \
             WHERE tenant_id = $1 \
               AND driver_id = $2 \
               AND (reading_date BETWEEN $3 AND $4 \
                    OR operation_date BETWEEN $3 AND $4) \
               AND has_kudgivt = TRUE \
             ORDER BY unko_no, crew_role ASC, departure_at NULLS LAST",
        )
        .bind(tenant_id)
        .bind(driver_id)
        .bind(from_widened)
        .bind(to_widened)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn list_drivers_with_operations(
        &self,
        tenant_id: Uuid,
        from: NaiveDate,
        to: NaiveDate,
        after_driver_cd: Option<&str>,
        limit: i64,
    ) -> Result<Vec<DtakoDriverRef>, sqlx::Error> {
        // list_operations と同じく暦日跨ぎのため ±1 日広げる
        let from_widened = from - chrono::Duration::days(1);
        let to_widened = to + chrono::Duration::days(1);
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, DtakoDriverRef>(
            "SELECT DISTINCT d.id AS driver_id, d.driver_cd, d.name AS driver_name \
             FROM alc_api.dtako_operations o \
             JOIN alc_api.employees d ON o.driver_id = d.id \
             WHERE o.tenant_id = $1 \
               AND (o.reading_date BETWEEN $2 AND $3 \
                    OR o.operation_date BETWEEN $2 AND $3) \
               AND o.has_kudgivt = TRUE \
               AND d.deleted_at IS NULL \
               AND d.driver_cd IS NOT NULL \
               AND ($4::TEXT IS NULL OR d.driver_cd > $4) \
             ORDER BY d.driver_cd \
             LIMIT $5",
        )
        .bind(tenant_id)
        .bind(from_widened)
        .bind(to_widened)
        .bind(after_driver_cd)
        .bind(limit)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn list_operations_for_drivers(
        &self,
        tenant_id: Uuid,
        driver_ids: &[Uuid],
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<DtakoDriverOperation>, sqlx::Error> {
        let from_widened = from - chrono::Duration::days(1);
        let to_widened = to + chrono::Duration::days(1);
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        // list_operations と同じ dedup (crew_role=1 優先) を乗務員ごとに行う
        sqlx::query_as::<_, DtakoDriverOperation>(
            "SELECT DISTINCT ON (driver_id, unko_no) \
                    driver_id, unko_no, crew_role, departure_at, return_at, r2_key_prefix \
             FROM alc_api.dtako_operations \
             WHERE tenant_id = $1 \
               AND driver_id = ANY($2) \
               AND (reading_date BETWEEN $3 AND $4 \
                    OR operation_date BETWEEN $3 AND $4) \
               AND has_kudgivt = TRUE \
             ORDER BY driver_id, unko_no, crew_role ASC, departure_at NULLS LAST",
        )
        .bind(tenant_id)
        .bind(driver_ids)
        .bind(from_widened)
        .bind(to_widened)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn list_unsplit_operations(
        &self,
        tenant_id: Uuid,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<Vec<UnsplitOperation>, sqlx::Error> {
        // 他 3 クエリと同じ ±1 日広げ (暦日跨ぎの運行取りこぼし防止)
        let from_widened = from - chrono::Duration::days(1);
        let to_widened = to + chrono::Duration::days(1);
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        // has_kudgivt だけ他 3 クエリと反転。employees の join は
        // list_drivers_with_operations と同じ経路 (driver_id = employees.id)。
        sqlx::query_as::<_, UnsplitOperation>(
            "SELECT o.unko_no, d.driver_cd, o.reading_date \
             FROM alc_api.dtako_operations o \
             JOIN alc_api.employees d ON o.driver_id = d.id \
             WHERE o.tenant_id = $1 \
               AND (o.reading_date BETWEEN $2 AND $3 \
                    OR o.operation_date BETWEEN $2 AND $3) \
               AND o.has_kudgivt = FALSE \
               AND d.driver_cd IS NOT NULL \
             ORDER BY o.unko_no",
        )
        .bind(tenant_id)
        .bind(from_widened)
        .bind(to_widened)
        .fetch_all(&mut *tc.conn)
        .await
    }
}

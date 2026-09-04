use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::{TimePunch, TimePunchWithDevice, TimecardCard};

use alc_core::tenant::TenantConn;

pub use alc_core::repository::timecard::*;

pub struct PgTimecardRepository {
    pool: PgPool,
}

impl PgTimecardRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Build dynamic WHERE clause and bind parameters.
/// Returns (where_clause, param_count_after).
fn build_punch_where(
    employee_id: Option<Uuid>,
    date_from: Option<DateTime<Utc>>,
    date_to: Option<DateTime<Utc>>,
    table_prefix: &str,
) -> (String, u32) {
    let mut conditions = vec![format!("{table_prefix}.tenant_id = $1")];
    let mut param_idx = 2u32;

    if employee_id.is_some() {
        conditions.push(format!("{table_prefix}.employee_id = ${param_idx}"));
        param_idx += 1;
    }
    if date_from.is_some() {
        conditions.push(format!("{table_prefix}.punched_at >= ${param_idx}"));
        param_idx += 1;
    }
    if date_to.is_some() {
        conditions.push(format!("{table_prefix}.punched_at <= ${param_idx}"));
        param_idx += 1;
    }

    (conditions.join(" AND "), param_idx)
}

/// 打刻一覧の共通 CTE (Refs ippoan/alc-app-s3#134)。
///
/// **打刻の一次表は `hub_measurements` で、`time_punches` は読まない。**
/// あちらはコピーであり、コピーを作ったせいで「時刻がサーバ時刻になる」
/// 「端末 ID が入らない」「重複排除が要る」が生まれた。書き手を外したので、
/// 読み出しも元の表へ寄せる。
///
/// # 社員の解決は 1 本の式
///
/// `payload.employee_id` (ingest で凍結済み) → カード登録 → 免許証番号、の順。
/// **`timecard` だけが凍結される**: `timecard_cards` は hard DELETE +
/// `UNIQUE (tenant_id, card_id)` でカードの付け替えが「削除 → 再登録」になるため、
/// 読むたびに解決すると**退職者のカードを新人に回した瞬間に退職者の過去の打刻が
/// 全部新人に付く**。`license` は免許証番号が人に固定で付け替え問題が無いので
/// 凍結せず毎回引く。分岐せず 1 本の COALESCE にしてあるので、経路ごとに実装が
/// 割れない。おまけとして**未解決だった timecard 行が、後からカードを登録すると
/// 自動で拾われる** (backfill が要らない)。
///
/// # `employee_id` は必ずパターン検証してから cast する
///
/// ingest は端末が名乗った `employee_id` を捨てる (`strip_client_employee_id`) が、
/// **#615 より前に入った行や、将来別経路で入った行が UUID でない値を持ちうる**。
/// 素で `::uuid` すると 1 行のせいで一覧全体が 500 になるので、正規表現で
/// 絞ってから cast する。
///
/// # card_id の正規化は `normalize_card_id` と同じ規則
///
/// `alc_core::repository::timecard::normalize_card_id` (trim + 小文字 + `':'` 除去)
/// の SQL 版。**片方だけ変えると照合が静かに外れる**ので、変えるときは両方同時に。
const PUNCHES_CTE: &str = r#"
WITH p AS (
    SELECT
        hm.id,
        hm.tenant_id,
        COALESCE(
            CASE WHEN hm.payload->>'employee_id'
                      ~ '^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$'
                 THEN (hm.payload->>'employee_id')::uuid END,
            tc.employee_id,
            e_nfc.id
        ) AS employee_id,
        hm.device_id AS hub_device_id,
        COALESCE(hm.recorded_at, hm.created_at) AS punched_at,
        hm.created_at
    FROM hub_measurements hm
    LEFT JOIN timecard_cards tc
           ON tc.tenant_id = hm.tenant_id
          AND tc.card_id = lower(replace(btrim(hm.payload->>'card_id'), ':', ''))
    LEFT JOIN employees e_nfc
           ON e_nfc.tenant_id = hm.tenant_id
          AND e_nfc.nfc_id = COALESCE(hm.payload->>'card_id', hm.payload->>'nfc_id')
    WHERE hm.tenant_id = $1 AND hm.kind IN ('timecard', 'license')
)
"#;

#[async_trait]
impl TimecardRepository for PgTimecardRepository {
    async fn create_card(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        card_id: &str,
        label: Option<&str>,
    ) -> Result<TimecardCard, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TimecardCard>(
            r#"
            INSERT INTO timecard_cards (tenant_id, employee_id, card_id, label)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(employee_id)
        .bind(card_id)
        .bind(label)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn list_cards(
        &self,
        tenant_id: Uuid,
        employee_id: Option<Uuid>,
    ) -> Result<Vec<TimecardCard>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        if let Some(eid) = employee_id {
            sqlx::query_as::<_, TimecardCard>(
                "SELECT * FROM timecard_cards WHERE tenant_id = $1 AND employee_id = $2 ORDER BY created_at",
            )
            .bind(tenant_id)
            .bind(eid)
            .fetch_all(&mut *tc.conn)
            .await
        } else {
            sqlx::query_as::<_, TimecardCard>(
                "SELECT * FROM timecard_cards WHERE tenant_id = $1 ORDER BY created_at",
            )
            .bind(tenant_id)
            .fetch_all(&mut *tc.conn)
            .await
        }
    }

    async fn get_card(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TimecardCard>(
            "SELECT * FROM timecard_cards WHERE id = $1 AND tenant_id = $2",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn get_card_by_card_id(
        &self,
        tenant_id: Uuid,
        card_id: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TimecardCard>(
            "SELECT * FROM timecard_cards WHERE tenant_id = $1 AND card_id = $2",
        )
        .bind(tenant_id)
        .bind(card_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn delete_card(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query("DELETE FROM timecard_cards WHERE id = $1 AND tenant_id = $2")
            .bind(id)
            .bind(tenant_id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn find_card_by_card_id(
        &self,
        tenant_id: Uuid,
        card_id: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        // Same as get_card_by_card_id — kept as alias for clarity in punch flow
        self.get_card_by_card_id(tenant_id, card_id).await
    }

    async fn find_employee_id_by_nfc(
        &self,
        tenant_id: Uuid,
        nfc_id: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM employees WHERE tenant_id = $1 AND nfc_id = $2",
        )
        .bind(tenant_id)
        .bind(nfc_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn create_punch(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        device_id: Option<Uuid>,
    ) -> Result<TimePunch, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TimePunch>(
            r#"
            INSERT INTO time_punches (tenant_id, employee_id, device_id)
            VALUES ($1, $2, $3)
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(employee_id)
        .bind(device_id)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn get_employee_name(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<String, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_scalar("SELECT name FROM employees WHERE id = $1 AND tenant_id = $2")
            .bind(employee_id)
            .bind(tenant_id)
            .fetch_one(&mut *tc.conn)
            .await
    }

    async fn list_today_punches(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<TimePunch>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TimePunch>(
            r#"
            SELECT * FROM time_punches
            WHERE tenant_id = $1 AND employee_id = $2
              AND punched_at >= CURRENT_DATE
            ORDER BY punched_at
            "#,
        )
        .bind(tenant_id)
        .bind(employee_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn count_punches(
        &self,
        tenant_id: Uuid,
        employee_id: Option<Uuid>,
        date_from: Option<DateTime<Utc>>,
        date_to: Option<DateTime<Utc>>,
    ) -> Result<i64, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let (where_clause, _) = build_punch_where(employee_id, date_from, date_to, "p");
        let count_sql = format!("{PUNCHES_CTE} SELECT COUNT(*) FROM p WHERE {where_clause}");

        let mut query = sqlx::query_scalar::<_, i64>(&count_sql).bind(tenant_id);
        if let Some(eid) = employee_id {
            query = query.bind(eid);
        }
        if let Some(df) = date_from {
            query = query.bind(df);
        }
        if let Some(dt) = date_to {
            query = query.bind(dt);
        }
        query.fetch_one(&mut *tc.conn).await
    }

    async fn list_punches(
        &self,
        tenant_id: Uuid,
        employee_id: Option<Uuid>,
        date_from: Option<DateTime<Utc>>,
        date_to: Option<DateTime<Utc>>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TimePunchWithDevice>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let (where_clause, param_idx) = build_punch_where(employee_id, date_from, date_to, "p");

        // device_id は常に NULL (hub の device_id は文字列で devices(id) の UUID FK に
        // 入らない)。どの端末かは device_name に入れた hub の device_id で追う
        let sql = format!(
            r#"{PUNCHES_CTE}
               SELECT p.id, p.tenant_id, p.employee_id, NULL::uuid AS device_id,
                      p.hub_device_id AS device_name,
                      e.name AS employee_name, p.punched_at, p.created_at
               FROM p
               LEFT JOIN employees e ON e.id = p.employee_id
               WHERE {where_clause}
               ORDER BY p.punched_at DESC LIMIT ${param_idx} OFFSET ${}"#,
            param_idx + 1
        );

        let mut query = sqlx::query_as::<_, TimePunchWithDevice>(&sql).bind(tenant_id);
        if let Some(eid) = employee_id {
            query = query.bind(eid);
        }
        if let Some(df) = date_from {
            query = query.bind(df);
        }
        if let Some(dt) = date_to {
            query = query.bind(dt);
        }
        query = query.bind(limit).bind(offset);

        query.fetch_all(&mut *tc.conn).await
    }

    async fn list_punches_for_csv(
        &self,
        tenant_id: Uuid,
        employee_id: Option<Uuid>,
        date_from: Option<DateTime<Utc>>,
        date_to: Option<DateTime<Utc>>,
    ) -> Result<Vec<TimePunchCsvRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let (where_clause, _) = build_punch_where(employee_id, date_from, date_to, "p");

        // employees は **LEFT** JOIN。INNER にすると未登録カードのタップが CSV から
        // 静かに消え、登録漏れに気付けなくなる (空欄で出す)
        let sql = format!(
            r#"{PUNCHES_CTE}
            SELECT p.id, p.punched_at, e.name AS employee_name, e.code AS employee_code,
                   p.hub_device_id AS device_name
            FROM p
            LEFT JOIN employees e ON e.id = p.employee_id
            WHERE {where_clause}
            ORDER BY p.punched_at DESC
            "#
        );

        let mut query = sqlx::query_as::<_, TimePunchCsvRow>(&sql).bind(tenant_id);
        if let Some(eid) = employee_id {
            query = query.bind(eid);
        }
        if let Some(df) = date_from {
            query = query.bind(df);
        }
        if let Some(dt) = date_to {
            query = query.bind(dt);
        }

        query.fetch_all(&mut *tc.conn).await
    }
}

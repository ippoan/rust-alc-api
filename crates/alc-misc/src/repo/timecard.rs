use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::{
    TimePunch, TimePunchWithDevice, TimecardCard, TimecardCardConflictPolicy,
    TimecardCardDeleteResult, TimecardCardUpsertSkipped, TimecardCardUpsertSummary,
};

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
/// **打刻の一次表は `hub_measurements` ただ 1 つ。**
/// かつては ingest のコピーを別表に持っており、そのせいで「時刻がサーバ時刻になる」
/// 「端末 ID が入らない」「重複排除が要る」が生まれた。書き手・読み手を外したうえで
/// 表ごと DROP した (#620) ので、経路は元の表 1 本だけである。
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
        -- 未解決のタップで「どのカードか」を出すため。employees の JOIN 条件と
        -- 同じ式なので、片方だけ変えると表示と照合がずれる
        COALESCE(hm.payload->>'card_id', hm.payload->>'nfc_id') AS card_id,
        hm.kind,
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

    /// 社員番号 (code) キーの一括 upsert (Refs ippoan/rust-alc-api#644)。
    ///
    /// 既存タイムカード (別システム) の中央 DB にあるカード台帳を、こちらへ
    /// 初回移行するための口。`items` は `prepare_bulk_cards` 通過後 = 正規化済み・
    /// 形が正しい・バッチ内で card_id が一意、が前提。
    ///
    /// **1 件の不備でトランザクションごと落とさない**のが眼目:
    /// 社員の解決は SELECT、書き込みは `ON CONFLICT` なので制約違反が起きる余地が無く、
    /// savepoint を使わずに「skip して次へ」が成り立つ。
    async fn bulk_upsert_cards_by_code(
        &self,
        tenant_id: Uuid,
        items: &[PreparedCardUpsert],
        on_conflict: TimecardCardConflictPolicy,
        dry_run: bool,
    ) -> Result<TimecardCardUpsertSummary, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        alc_core::tenant::set_current_tenant(&mut tx, &tenant_id.to_string()).await?;

        let reassign = on_conflict == TimecardCardConflictPolicy::Reassign;
        let mut summary = TimecardCardUpsertSummary::default();

        for item in items {
            // 社員の解決。**`deleted_at` は問わない** (`upsert_by_code` と同じ理由 —
            // `idx_employees_code` が deleted_at を見ない一意制約なので、削除済みの
            // 行を無視すると「居ないのに code が使われている」状態になる)
            let employee: Option<(Uuid,)> =
                sqlx::query_as("SELECT id FROM employees WHERE tenant_id = $1 AND code = $2")
                    .bind(tenant_id)
                    .bind(&item.code)
                    .fetch_optional(&mut *tx)
                    .await?;

            let Some((employee_id,)) = employee else {
                summary.skipped.push(TimecardCardUpsertSkipped {
                    index: item.index,
                    code: item.code.clone(),
                    reason: "employee_not_found".to_string(),
                });
                continue;
            };

            // `xmax = 0` で INSERT と UPDATE を見分ける。DO UPDATE の WHERE が
            // 外れた (= 既に同じ社員に付いている / 付け替えない指定) ときは
            // 1 行も返らないので、その 3 通りを 1 文で分けられる。
            //
            // ★ **`DO UPDATE` の SET に `source` を足さないこと** (Refs ippoan/rust-alc-api#644)。
            // 足すと「同じ持ち主だから何もしない」経路が 1 行も書かない前提が崩れ、
            // 実測済みの冪等性 (2 回目が `created:0 / unchanged:120`) が後退する。
            // その帰結として **alc 側で先に登録された行 (`source IS NULL`) は、同期が
            // 同じ社員のまま再確認しても NULL のまま = 削除の射程外**になるが、
            // これは fail-closed 側 (消せないだけ) なので仕様。
            // `tests/timecard_cards_bulk_test.rs` の
            // `test_unchanged_row_keeps_null_source_by_design` が固定している。
            let written: Option<(bool,)> = sqlx::query_as(
                r#"
                INSERT INTO timecard_cards (tenant_id, employee_id, card_id, label, source)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT (tenant_id, card_id) DO UPDATE
                    SET employee_id = EXCLUDED.employee_id
                    WHERE $6::boolean AND timecard_cards.employee_id <> EXCLUDED.employee_id
                RETURNING (xmax = 0) AS inserted
                "#,
            )
            .bind(tenant_id)
            .bind(employee_id)
            .bind(&item.card_id)
            .bind(&item.label)
            // ★ 出所はサーバ側の定数。**body から取らない** — 削除
            // (`POST /timecard/cards/delete-by-card`) の射程を決める値なので、
            // 送り手が書けると alc 側で直接登録したカードまで消せるようになる
            .bind(CARD_SOURCE_LEDGER_SYNC)
            .bind(reassign)
            .fetch_optional(&mut *tx)
            .await?;

            match written {
                Some((true,)) => summary.created += 1,
                Some((false,)) => summary.updated += 1,
                None => {
                    // 書かなかった = 同じ card_id の行が既にある。持ち主が同じなら
                    // 何もしないのが正 (unchanged)、別人なら見送った行
                    let owner: Option<(Uuid,)> = sqlx::query_as(
                        "SELECT employee_id FROM timecard_cards WHERE tenant_id = $1 AND card_id = $2",
                    )
                    .bind(tenant_id)
                    .bind(&item.card_id)
                    .fetch_optional(&mut *tx)
                    .await?;

                    if owner == Some((employee_id,)) {
                        summary.unchanged += 1;
                    } else {
                        summary.skipped.push(TimecardCardUpsertSkipped {
                            index: item.index,
                            code: item.code.clone(),
                            reason: "card_owner_conflict".to_string(),
                        });
                    }
                }
            }
        }

        // dry_run は**判定を最後まで同じコードで通してから commit しない**形。
        // 判定を 2 本持つと「試したときと本番で結果が違う」が必ず生まれる
        if dry_run {
            tx.rollback().await?;
        } else {
            tx.commit().await?;
        }

        Ok(summary)
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

    /// 同期で入れたカード 1 枚を外す (Refs ippoan/rust-alc-api#644)。
    ///
    /// **判定も削除も 1 文で当てる。** 「あるか調べてから消す」だと、間に
    /// 付け替え (`bulk-by-code` の reassign) や別経路の削除が入り得る。
    /// CTE の中では `gone` の DELETE の結果が `present` からは見えない
    /// (同じスナップショットを見る) ので、次の 3 通りが 1 往復で分かれる:
    ///
    /// * `gone` に行がある → 消えた (`deleted`)。`code` は**誰のカードを外したか**
    /// * `gone` は空で `present` に行がある → 出所が違う (`out_of_scope`)
    /// * どちらも空 → そもそも無い (`not_found`)
    async fn delete_card_by_card_id_from_sync(
        &self,
        tenant_id: Uuid,
        card_id: &str,
        dry_run: bool,
    ) -> Result<TimecardCardDeleteResult, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        alc_core::tenant::set_current_tenant(&mut tx, &tenant_id.to_string()).await?;

        let (deleted, present, code): (bool, bool, Option<String>) = sqlx::query_as(
            r#"
            WITH gone AS (
                DELETE FROM timecard_cards
                WHERE tenant_id = $1 AND card_id = $2 AND source = $3
                RETURNING employee_id
            ),
            present AS (
                SELECT 1 FROM timecard_cards WHERE tenant_id = $1 AND card_id = $2
            )
            SELECT
                EXISTS (SELECT 1 FROM gone)    AS deleted,
                EXISTS (SELECT 1 FROM present) AS present,
                (SELECT e.code FROM gone JOIN employees e ON e.id = gone.employee_id) AS code
            "#,
        )
        .bind(tenant_id)
        .bind(card_id)
        // ★ 射程はサーバ側の定数ちょうど。alc 側で直接登録した行 (source IS NULL) は
        // `= $3` に当たらないので巻き込まない
        .bind(CARD_SOURCE_LEDGER_SYNC)
        .fetch_one(&mut *tx)
        .await?;

        // dry_run は**判定を最後まで同じコードで通してから commit しない**形
        // (`bulk_upsert_cards_by_code` と同じ理由 — 判定を 2 本持つと
        // 「試したときと本番で結果が違う」が必ず生まれる)
        if dry_run {
            tx.rollback().await?;
        } else {
            tx.commit().await?;
        }

        Ok(match (deleted, present) {
            (true, _) => TimecardCardDeleteResult {
                deleted: 1,
                reason: "deleted".to_string(),
                code,
            },
            (false, true) => TimecardCardDeleteResult {
                deleted: 0,
                reason: "out_of_scope".to_string(),
                code: None,
            },
            (false, false) => TimecardCardDeleteResult {
                deleted: 0,
                reason: "not_found".to_string(),
                code: None,
            },
        })
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
        card_id: &str,
    ) -> Result<TimePunch, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        // device_id は hub_measurements では TEXT NOT NULL。キオスクの UUID が
        // 無ければ 'browser' に倒す (端末の device_id とは名前空間が衝突しない)
        let hub_device_id = device_id
            .map(|d| d.to_string())
            .unwrap_or_else(|| "browser".to_string());
        // employee_id は payload に凍結する (端末側 freeze_employee_id と同じ)。
        // recorded_at は now() — ブラウザは自前の計時を送ってこない
        sqlx::query_as::<_, TimePunch>(
            r#"
            INSERT INTO hub_measurements (tenant_id, device_id, kind, payload, seq, recorded_at)
            VALUES (
                $1, $2, 'timecard',
                jsonb_build_object('card_id', $4::text, 'employee_id', $3::text),
                nextval('hub_measurements_browser_seq'), now()
            )
            RETURNING id, tenant_id, $3::uuid AS employee_id, NULL::uuid AS device_id,
                      recorded_at AS punched_at, created_at
            "#,
        )
        .bind(tenant_id)
        .bind(&hub_device_id)
        .bind(employee_id)
        .bind(card_id)
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
        // 打刻の一次表は hub_measurements (Refs ippoan/alc-app-s3#134)。一覧と
        // 同じ CTE を使う — ここだけ別の表を読むと、ブラウザで打った直後の応答に
        // その打刻が出ない (書き込み先が違うため)。
        //
        // **「今日」は JST で切る。** `CURRENT_DATE` はサーバ TZ (Cloud Run は UTC) の
        // 日付なので、JST の 0 時〜9 時に打刻すると「昨日」に落ちて当日一覧から消える。
        // 表示側 (CSV / 画面) は既に JST 固定なので、境界だけ UTC のままだった。
        let sql = format!(
            r#"{PUNCHES_CTE}
            SELECT p.id, p.tenant_id, p.employee_id, NULL::uuid AS device_id,
                   p.punched_at, p.created_at
            FROM p
            WHERE p.tenant_id = $1 AND p.employee_id = $2
              AND p.punched_at >= ((now() AT TIME ZONE 'Asia/Tokyo')::date::timestamp
                                   AT TIME ZONE 'Asia/Tokyo')
            ORDER BY p.punched_at
            "#
        );
        sqlx::query_as::<_, TimePunch>(&sql)
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
                      p.hub_device_id AS device_name, p.card_id, p.kind,
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
                   p.hub_device_id AS device_name, p.kind
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

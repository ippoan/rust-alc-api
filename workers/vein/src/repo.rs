//! Worker の `VeinTemplatesRepository` 実装 (tokio-postgres)。SQL は alc-vein の `repo::sql`
//! (モノリスの sqlx 実装と同じもの) を使う。接続は [`crate::db::connect`] が張る。
//!
//! ## RLS (trait のメソッド 1 回 = 1 トランザクション)
//!
//! DB の前には transaction mode のプーラー (staging は Container 内の PgBouncer、本番は
//! Supabase のプーラー) が入り、トランザクション単位で上流のコネクションを使い回す。
//! monolith の `set_current_tenant` (migrations/004・062、`set_config(.., false)` = session スコープ)
//! をそのまま打つと、tenant が別リクエスト (別テナント) へ漏れるか、次の文が別の
//! コネクションに載って行ゼロになる。だから必ず `BEGIN` の中で
//! `set_config('app.current_tenant_id', $1, true)` (= `SET LOCAL`、COMMIT/ROLLBACK で消える)
//! を打ち、同じトランザクションの中でクエリを流す。
//!
//! `set_current_tenant` は `set_config` を包むだけ (検証なし) で、SECURITY DEFINER も
//! custom GUC の設定に権限が要らないので効いていない。第 3 引数を `true` にした
//! `set_config` を直接打つのと、スコープ以外は同じ。
//!
//! search_path も同じ `SELECT` で `SET LOCAL` 相当にする (プーラーが接続文字列の
//! `options=-c search_path=..` を上流へ渡す保証が無く、DB 既定に頼らないため)。
//!
//! メソッドごとにトランザクションを閉じるので、`POST /vein/identify` は
//! 「tx1 で一覧 → トランザクションの外で照合 → tx2 で学習の書き戻し」になり、照合の CPU の間
//! プーラーのコネクションを握らない。

use std::sync::atomic::{AtomicU64, Ordering};

use alc_core_wasm::DbError;
use alc_vein::repo::{sql, VeinTemplateRow, VeinTemplatesRepository};
use chrono::{DateTime, Utc};
use futures_util::lock::Mutex;
use tokio_postgres::{Client, Transaction};
use uuid::Uuid;
use worker::{console_error, Date};

/// PostgreSQL の unique_violation (alc-core-wasm の sqlx 版と同じ写し方)。
const PG_UNIQUE_VIOLATION: &str = "23505";

fn db_err(e: tokio_postgres::Error) -> DbError {
    // tokio_postgres::Error の Display は "db error" だけなので、DB の message も載せる
    let err = match e.as_db_error() {
        Some(db) if db.code().code() == PG_UNIQUE_VIOLATION => {
            DbError::Conflict(db.message().to_string())
        }
        Some(db) => DbError::Other(format!("{} ({})", db.message(), db.code().code())),
        None => DbError::Other(e.to_string()),
    };
    // routes は 500 の中身を返さないので、原因はここで Workers のログに残す
    console_error!("vein repo: {err:?}");
    err
}

/// 1 リクエストぶんの repo。接続 (`Client`) は handler の外 (fetch) で張って渡す。
/// `Client` は `Send` なので `SendWrapper` は要らない。トランザクションに `&mut Client` が
/// 要るので async の Mutex で包む (Workers は単一スレッドなので競合はしない)。
pub struct WorkerVeinTemplatesRepository {
    client: Mutex<Client>,
    /// DB に使った時間の合計 (ms)。fetch がリクエスト全体から引いて Server-Timing に載せる
    db_ms: AtomicU64,
}

impl WorkerVeinTemplatesRepository {
    pub fn new(client: Client) -> Self {
        Self {
            client: Mutex::new(client),
            db_ms: AtomicU64::new(0),
        }
    }

    pub fn db_ms(&self) -> u64 {
        self.db_ms.load(Ordering::Relaxed)
    }

    fn add_db_ms(&self, started: u64) {
        let spent = Date::now().as_millis().saturating_sub(started);
        self.db_ms.fetch_add(spent, Ordering::Relaxed);
    }
}

/// tenant を `SET LOCAL` したトランザクションを始める。
///
/// **`Row` は COMMIT の前に取り出して捨てること。** `tokio_postgres::Row` は prepared statement
/// (`s0`, `s1`, ...) を握っていて、最後の `Row` が drop された時点で Close を送る。COMMIT の後に
/// drop すると Close がトランザクションの外に出て、transaction mode のプーラーでは別のサーバー接続へ
/// 回り、元の接続に `s1` が残って次の利用者が `prepared statement "s1" already exists` で落ちる
/// (staging の PgBouncer で実測、Refs #691)。`Transaction` は drop で
/// ROLLBACK されるので、呼び出し側が `commit` する。
async fn tenant_tx(client: &mut Client, tenant_id: Uuid) -> Result<Transaction<'_>, DbError> {
    let tx = client.transaction().await.map_err(db_err)?;
    tx.execute(
        "SELECT set_config('app.current_tenant_id', $1, true), set_config('search_path', 'alc_api', true)",
        &[&tenant_id.to_string()],
    )
    .await
    .map_err(db_err)?;
    Ok(tx)
}

#[async_trait::async_trait]
impl VeinTemplatesRepository for WorkerVeinTemplatesRepository {
    async fn upsert(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        template: &str,
    ) -> Result<Option<DateTime<Utc>>, DbError> {
        let started = Date::now().as_millis();
        let mut client = self.client.lock().await;
        let tx = tenant_tx(&mut client, tenant_id).await?;
        let updated_at = tx
            .query_opt(sql::UPSERT, &[&tenant_id, &employee_id, &template])
            .await
            .map_err(db_err)?
            .map(|r| r.get(0));
        tx.commit().await.map_err(db_err)?;
        self.add_db_ms(started);
        Ok(updated_at)
    }

    async fn list(&self, tenant_id: Uuid) -> Result<Vec<VeinTemplateRow>, DbError> {
        let started = Date::now().as_millis();
        let mut client = self.client.lock().await;
        let tx = tenant_tx(&mut client, tenant_id).await?;
        let rows = tx
            .query(sql::LIST, &[&tenant_id])
            .await
            .map_err(db_err)?
            .into_iter()
            .map(|r| VeinTemplateRow {
                id: r.get(0),
                employee_id: r.get(1),
                name: r.get(2),
                template: r.get(3),
                updated_at: r.get(4),
            })
            .collect();
        tx.commit().await.map_err(db_err)?;
        self.add_db_ms(started);
        Ok(rows)
    }

    async fn registration_count(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<(i64, bool), DbError> {
        let started = Date::now().as_millis();
        let mut client = self.client.lock().await;
        let tx = tenant_tx(&mut client, tenant_id).await?;
        let row = tx
            .query_one(sql::REGISTRATION_COUNT, &[&tenant_id, &employee_id])
            .await
            .map_err(db_err)?;
        let counts = (row.get(0), row.get(1));
        drop(row);
        tx.commit().await.map_err(db_err)?;
        self.add_db_ms(started);
        Ok(counts)
    }

    async fn update_learned(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        template: &str,
        read_updated_at: DateTime<Utc>,
    ) -> Result<bool, DbError> {
        let started = Date::now().as_millis();
        let mut client = self.client.lock().await;
        let tx = tenant_tx(&mut client, tenant_id).await?;
        let n = tx
            .execute(
                sql::UPDATE_LEARNED,
                &[&tenant_id, &id, &template, &read_updated_at],
            )
            .await
            .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;
        self.add_db_ms(started);
        Ok(n == 1)
    }

    async fn delete(&self, tenant_id: Uuid, employee_id: Uuid) -> Result<bool, DbError> {
        let started = Date::now().as_millis();
        let mut client = self.client.lock().await;
        let tx = tenant_tx(&mut client, tenant_id).await?;
        let n = tx
            .execute(sql::DELETE, &[&tenant_id, &employee_id])
            .await
            .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;
        self.add_db_ms(started);
        Ok(n == 1)
    }
}

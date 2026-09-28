//! Hyperdrive 越しの `vein_templates` の読み書き (段階 A 限定の最小実装。段階 B で
//! crates/alc-vein の trait を実装した `HdVeinTemplatesRepository` に置き換える、Refs #682)。
//! SQL は crates/alc-vein/src/repo.rs と同じ。
//!
//! ## RLS (1 リクエスト = 1 トランザクション)
//!
//! Hyperdrive はトランザクション単位で上流のコネクションを使い回すので、monolith の
//! `set_current_tenant` (migrations/004・062、`set_config(.., false)` = session スコープ)
//! をそのまま打つと、tenant が別リクエスト (別テナント) へ漏れるか、次の文が別の
//! コネクションに載って行ゼロになる。だから必ず `BEGIN` の中で
//! `set_config('app.current_tenant_id', $1, true)` (= `SET LOCAL`、COMMIT/ROLLBACK で消える)
//! を打ち、同じトランザクションの中でクエリを流す。
//!
//! `set_current_tenant` は `set_config` を包むだけ (検証なし) で、SECURITY DEFINER も
//! custom GUC の設定に権限が要らないので効いていない。第 3 引数を `true` にした
//! `set_config` を直接打つのと、スコープ以外は同じ。
//!
//! search_path も同じ `SELECT` で `SET LOCAL` 相当にする (Hyperdrive は接続文字列の
//! `options=-c search_path=..` を上流へ渡す保証が無く、DB 既定に頼らないため)。

use chrono::{DateTime, Utc};
use tokio_postgres::config::SslMode;
use tokio_postgres::{Client, NoTls, Transaction};
use uuid::Uuid;
use worker::{console_error, Env, Socket};

pub type DbError = String;

fn db_err(context: &str) -> impl Fn(tokio_postgres::Error) -> DbError + '_ {
    // tokio_postgres::Error の Display は "db error" だけなので、DB の message も載せる
    move |e| match e.as_db_error() {
        Some(db) => format!("{context}: {} ({})", db.message(), db.code().code()),
        None => format!("{context}: {e}"),
    }
}

/// 照合に使う 1 行 (乗務員名つき。削除済みの乗務員は含めない)。
pub struct VeinTemplateRow {
    pub id: Uuid,
    pub employee_id: Uuid,
    pub name: String,
    pub template: String,
    pub updated_at: DateTime<Utc>,
}

/// Hyperdrive binding `HYPERDRIVE` へ繋ぐ。Hyperdrive までは Cloudflare 内で TLS は
/// Hyperdrive が上流へ張るので、worker → Hyperdrive は平文 (`NoTls`)。
pub async fn connect(env: &Env) -> Result<Client, DbError> {
    let hd = env
        .hyperdrive("HYPERDRIVE")
        .map_err(|e| format!("hyperdrive binding: {e}"))?;
    let mut config: tokio_postgres::Config = hd
        .connection_string()
        .parse()
        .map_err(db_err("parse connection string"))?;
    config.ssl_mode(SslMode::Disable);
    let socket = Socket::builder()
        .connect(hd.host(), hd.port())
        .map_err(|e| format!("socket: {e}"))?;
    let (client, connection) = config
        .connect_raw(socket, NoTls)
        .await
        .map_err(db_err("connect"))?;
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = connection.await {
            console_error!("postgres connection: {e}");
        }
    });
    Ok(client)
}

/// tenant を `SET LOCAL` したトランザクションを始める。`Transaction` は drop で
/// ROLLBACK されるので、書き込みは呼び出し側が `commit` する。
pub async fn tenant_tx(client: &mut Client, tenant_id: Uuid) -> Result<Transaction<'_>, DbError> {
    let tx = client.transaction().await.map_err(db_err("begin"))?;
    tx.execute(
        "SELECT set_config('app.current_tenant_id', $1, true), set_config('search_path', 'alc_api', true)",
        &[&tenant_id.to_string()],
    )
    .await
    .map_err(db_err("set tenant"))?;
    Ok(tx)
}

pub async fn upsert(
    tx: &Transaction<'_>,
    tenant_id: Uuid,
    employee_id: Uuid,
    template: &str,
) -> Result<Option<DateTime<Utc>>, DbError> {
    let row = tx
        .query_opt(
            r#"INSERT INTO vein_templates (tenant_id, employee_id, template)
            SELECT e.tenant_id, e.id, $3 FROM employees e
            WHERE e.tenant_id = $1 AND e.id = $2 AND e.deleted_at IS NULL
            ON CONFLICT (tenant_id, employee_id)
            DO UPDATE SET template = EXCLUDED.template, updated_at = NOW()
            RETURNING updated_at"#,
            &[&tenant_id, &employee_id, &template],
        )
        .await
        .map_err(db_err("vein upsert"))?;
    Ok(row.map(|r| r.get(0)))
}

pub async fn list(tx: &Transaction<'_>, tenant_id: Uuid) -> Result<Vec<VeinTemplateRow>, DbError> {
    let rows = tx
        .query(
            r#"SELECT v.id, v.employee_id, e.name, v.template, v.updated_at
            FROM vein_templates v
            JOIN employees e ON e.id = v.employee_id AND e.tenant_id = v.tenant_id
            WHERE v.tenant_id = $1 AND e.deleted_at IS NULL
            ORDER BY v.created_at, v.id"#,
            &[&tenant_id],
        )
        .await
        .map_err(db_err("vein list"))?;
    Ok(rows
        .into_iter()
        .map(|r| VeinTemplateRow {
            id: r.get(0),
            employee_id: r.get(1),
            name: r.get(2),
            template: r.get(3),
            updated_at: r.get(4),
        })
        .collect())
}

pub async fn registration_count(
    tx: &Transaction<'_>,
    tenant_id: Uuid,
    employee_id: Uuid,
) -> Result<(i64, bool), DbError> {
    let row = tx
        .query_one(
            r#"SELECT COUNT(*), COALESCE(BOOL_OR(v.employee_id = $2), FALSE)
            FROM vein_templates v
            JOIN employees e ON e.id = v.employee_id AND e.tenant_id = v.tenant_id
            WHERE v.tenant_id = $1 AND e.deleted_at IS NULL"#,
            &[&tenant_id, &employee_id],
        )
        .await
        .map_err(db_err("vein registration_count"))?;
    Ok((row.get(0), row.get(1)))
}

pub async fn update_learned(
    tx: &Transaction<'_>,
    tenant_id: Uuid,
    id: Uuid,
    template: &str,
    read_updated_at: DateTime<Utc>,
) -> Result<bool, DbError> {
    let n = tx
        .execute(
            r#"UPDATE vein_templates SET template = $3, updated_at = NOW()
            WHERE tenant_id = $1 AND id = $2 AND updated_at = $4"#,
            &[&tenant_id, &id, &template, &read_updated_at],
        )
        .await
        .map_err(db_err("vein update_learned"))?;
    Ok(n == 1)
}

pub async fn delete(
    tx: &Transaction<'_>,
    tenant_id: Uuid,
    employee_id: Uuid,
) -> Result<bool, DbError> {
    let n = tx
        .execute(
            "DELETE FROM vein_templates WHERE tenant_id = $1 AND employee_id = $2",
            &[&tenant_id, &employee_id],
        )
        .await
        .map_err(db_err("vein delete"))?;
    Ok(n == 1)
}

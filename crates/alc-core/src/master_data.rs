//! テナントごとに追加できる「マスタ」データの共通 CRUD (Refs ippoan/rust-alc-api#651)。
//!
//! `alc-trouble` の 4 本 (categories/offices/progress_statuses/task_types) が
//! 同じ形のテーブル (`id / tenant_id / name / sort_order / created_at` +
//! `UNIQUE (tenant_id, name)`) に対して同じ CRUD (list は
//! `ORDER BY sort_order, name`、create は `sort_order` 既定 0、update/delete は
//! `tenant_id` で絞った上で `RETURNING *`) を逐語複製していたため、テーブル名
//! だけ差し替えられる形でここへ寄せた。
//!
//! テーブル名はプレースホルダに bind できない (識別子は bind パラメータでは
//! 表せない) ので、SQL に文字列で埋め込むほかない。**ここが SQL injection の
//! 境界になる**。境界を保つため、テーブル名の入り口を
//! [`MasterTable::TABLE`] という**関連定数**だけに絞っている — 呼び出し側は
//! この trait を実装した具体型 (典型的には `PgXxxRepository` 自身) を型引数に
//! 渡すことでしか [`list`] 等を呼べず、実行時の文字列 (リクエストボディ・
//! パスパラメータ等) を `TABLE` に流し込む経路は存在しない。
//! **`TABLE` にはソースコード上のリテラル以外を割り当てないこと** — 動的に
//! 組み立てた文字列を代入すると、この前提が壊れる。

use chrono::{DateTime, Utc};
use sqlx::{postgres::PgRow, FromRow, PgPool};
use uuid::Uuid;

use crate::tenant::TenantConn;

/// `list`/`create`/`update_sort_order`/`delete` がどのテーブルに対して動くかを
/// コンパイル時に固定する。実装は「各利用者 (`PgXxxRepository`) が
/// `const TABLE: &str` を持つ」形にすること。
pub trait MasterTable {
    const TABLE: &'static str;
}

/// [`create`] が受け取る入力型に要求する最小限のフィールド。
pub trait MasterCreateInput {
    fn name(&self) -> &str;
    fn sort_order(&self) -> Option<i32>;
}

/// Mock 実装 (`tests/mock_helpers`) が list/create/update_sort_order の
/// ロジックを共通化するための、Row 型に求める最小限の操作 (Refs #651)。
/// 本番の Pg 実装は SQL の `RETURNING *` (`FromRow`) で組み立てるので
/// 使わない — こちらは実 DB を持たない Mock (in-memory `Vec<Row>`) 専用。
pub trait MasterRow: Sized {
    fn master_id(&self) -> Uuid;
    fn set_master_sort_order(&mut self, sort_order: i32);
    fn new_master_row(
        id: Uuid,
        tenant_id: Uuid,
        name: String,
        sort_order: i32,
        created_at: DateTime<Utc>,
    ) -> Self;
}

fn list_sql(table: &str) -> String {
    format!("SELECT * FROM {table} WHERE tenant_id = $1 ORDER BY sort_order, name")
}

fn insert_sql(table: &str) -> String {
    format!("INSERT INTO {table} (tenant_id, name, sort_order) VALUES ($1, $2, $3) RETURNING *")
}

fn delete_sql(table: &str) -> String {
    format!("DELETE FROM {table} WHERE id = $1 AND tenant_id = $2")
}

fn update_sort_order_sql(table: &str) -> String {
    format!("UPDATE {table} SET sort_order = $3 WHERE id = $1 AND tenant_id = $2 RETURNING *")
}

pub async fn list<M, R>(pool: &PgPool, tenant_id: Uuid) -> Result<Vec<R>, sqlx::Error>
where
    M: MasterTable,
    R: for<'r> FromRow<'r, PgRow> + Send + Unpin,
{
    let mut tc = TenantConn::acquire(pool, &tenant_id.to_string()).await?;
    sqlx::query_as::<_, R>(&list_sql(M::TABLE))
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
}

pub async fn create<M, R, I>(pool: &PgPool, tenant_id: Uuid, input: &I) -> Result<R, sqlx::Error>
where
    M: MasterTable,
    R: for<'r> FromRow<'r, PgRow> + Send + Unpin,
    I: MasterCreateInput + Sync,
{
    let mut tc = TenantConn::acquire(pool, &tenant_id.to_string()).await?;
    sqlx::query_as::<_, R>(&insert_sql(M::TABLE))
        .bind(tenant_id)
        .bind(input.name())
        .bind(input.sort_order().unwrap_or(0))
        .fetch_one(&mut *tc.conn)
        .await
}

pub async fn delete<M>(pool: &PgPool, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>
where
    M: MasterTable,
{
    let mut tc = TenantConn::acquire(pool, &tenant_id.to_string()).await?;
    let result = sqlx::query(&delete_sql(M::TABLE))
        .bind(id)
        .bind(tenant_id)
        .execute(&mut *tc.conn)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn update_sort_order<M, R>(
    pool: &PgPool,
    tenant_id: Uuid,
    id: Uuid,
    sort_order: i32,
) -> Result<Option<R>, sqlx::Error>
where
    M: MasterTable,
    R: for<'r> FromRow<'r, PgRow> + Send + Unpin,
{
    let mut tc = TenantConn::acquire(pool, &tenant_id.to_string()).await?;
    sqlx::query_as::<_, R>(&update_sort_order_sql(M::TABLE))
        .bind(id)
        .bind(tenant_id)
        .bind(sort_order)
        .fetch_optional(&mut *tc.conn)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeMaster;
    impl MasterTable for FakeMaster {
        const TABLE: &'static str = "fake_master_table";
    }

    struct FakeCreateInput {
        name: String,
        sort_order: Option<i32>,
    }
    impl MasterCreateInput for FakeCreateInput {
        fn name(&self) -> &str {
            &self.name
        }
        fn sort_order(&self) -> Option<i32> {
            self.sort_order
        }
    }

    // list/create/delete/update_sort_order 自体は TenantConn::acquire 経由で
    // 実 DB (RLS の set_current_tenant) を要求するため、DB を要する検証は
    // 呼び出し側 (alc-trouble の Pg*Repository) を通じて既存の
    // tests/trouble_test.rs (bazel-test-db) 側で担保する。ここでは DB 無しで
    // 検証できる「テーブル名の埋め込み方」と「入力のデフォルト値」だけを見る。

    #[test]
    fn list_sql_orders_by_sort_order_then_name() {
        assert_eq!(
            list_sql(FakeMaster::TABLE),
            "SELECT * FROM fake_master_table WHERE tenant_id = $1 ORDER BY sort_order, name"
        );
    }

    #[test]
    fn insert_sql_returns_all_columns() {
        let sql = insert_sql(FakeMaster::TABLE);
        assert!(sql.starts_with("INSERT INTO fake_master_table (tenant_id, name, sort_order)"));
        assert!(sql.trim_end().ends_with("RETURNING *"));
    }

    #[test]
    fn delete_sql_scopes_by_id_and_tenant() {
        assert_eq!(
            delete_sql(FakeMaster::TABLE),
            "DELETE FROM fake_master_table WHERE id = $1 AND tenant_id = $2"
        );
    }

    #[test]
    fn update_sort_order_sql_scopes_and_returns_row() {
        assert_eq!(
            update_sort_order_sql(FakeMaster::TABLE),
            "UPDATE fake_master_table SET sort_order = $3 WHERE id = $1 AND tenant_id = $2 RETURNING *"
        );
    }

    #[test]
    fn create_input_missing_sort_order_defaults_to_zero() {
        let input = FakeCreateInput {
            name: "x".to_string(),
            sort_order: None,
        };
        assert_eq!(input.sort_order().unwrap_or(0), 0);
    }

    #[test]
    fn create_input_keeps_explicit_sort_order() {
        let input = FakeCreateInput {
            name: "x".to_string(),
            sort_order: Some(7),
        };
        assert_eq!(input.sort_order().unwrap_or(0), 7);
    }

    #[derive(Clone)]
    struct FakeRow {
        id: Uuid,
        sort_order: i32,
    }
    impl MasterRow for FakeRow {
        fn master_id(&self) -> Uuid {
            self.id
        }
        fn set_master_sort_order(&mut self, sort_order: i32) {
            self.sort_order = sort_order;
        }
        fn new_master_row(
            id: Uuid,
            _tenant_id: Uuid,
            _name: String,
            sort_order: i32,
            _created_at: DateTime<Utc>,
        ) -> Self {
            Self { id, sort_order }
        }
    }

    #[test]
    fn master_row_new_carries_id_and_sort_order() {
        let id = Uuid::new_v4();
        let row = FakeRow::new_master_row(id, Uuid::new_v4(), "x".to_string(), 3, Utc::now());
        assert_eq!(row.master_id(), id);
        assert_eq!(row.sort_order, 3);
    }

    #[test]
    fn master_row_set_master_sort_order_updates_in_place() {
        let mut row = FakeRow {
            id: Uuid::new_v4(),
            sort_order: 0,
        };
        row.set_master_sort_order(9);
        assert_eq!(row.sort_order, 9);
    }
}

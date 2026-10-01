use sqlx::PgPool;

/// Set the current tenant for RLS policies.
/// Must be called before any tenant-scoped query.
///
/// dev端末の要求 (`device_dev::is_device_dev()`) では、続けて接続の設定
/// `app.device_dev` を `'1'` にする (Refs ippoan/alc-app#387)。列の既定値と RLS が
/// それを見て dev の行だけを読み書きさせる (alc-migrations 153)。
/// **順番は SQL の `set_current_tenant` の後** — あの関数は呼ばれるたびに
/// `app.device_dev` を `''` に戻すので、逆の順だと dev にならない。dev でないときは
/// 追加の文を打たない (関数が既に `''` にしている)。
pub async fn set_current_tenant(
    conn: &mut sqlx::PgConnection,
    tenant_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT set_current_tenant($1)")
        .bind(tenant_id)
        .execute(&mut *conn)
        .await?;
    if crate::device_dev::is_device_dev() {
        sqlx::query("SELECT set_config('app.device_dev', '1', false)")
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

/// プールへ返す接続から、テナントと dev端末の印を消す (`after_release` 用)。
///
/// どちらも session スコープ (`set_config(..., false)`) で立つので、返却しても
/// 残る。次に同じ接続を借りた別の要求 (特に `set_current_tenant` を通らない
/// `self.pool` 直クエリ) へ漏らさないため、未設定に戻す。Refs #386 / ippoan/alc-app#387。
pub async fn reset_tenant_context(conn: &mut sqlx::PgConnection) -> Result<(), sqlx::Error> {
    // 引数なしの文字列は simple query で送られるので、2 文を 1 往復で打てる
    sqlx::Executor::execute(conn, "RESET app.current_tenant_id; RESET app.device_dev").await?;
    Ok(())
}

/// テナントスコープの DB コネクション
/// acquire 時に set_current_tenant を自動呼び出しする
pub struct TenantConn {
    pub conn: sqlx::pool::PoolConnection<sqlx::Postgres>,
}

impl TenantConn {
    pub async fn acquire(pool: &PgPool, tenant_id: &str) -> Result<Self, sqlx::Error> {
        let mut conn = pool.acquire().await?;
        set_current_tenant(&mut conn, tenant_id).await?;
        Ok(Self { conn })
    }
}

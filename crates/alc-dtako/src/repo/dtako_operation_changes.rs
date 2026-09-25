//! `dtako_operation_changes` (運行の変更記録、migration 152) を読み書きする SQL。
//! 上げ直し (`PgDtakoUploadRepository::replace_operation`) と手動削除
//! (`PgDtakoOperationsRepository::delete_by_unko_no`) が同じトランザクションの中から呼ぶ。

use sqlx::PgConnection;
use uuid::Uuid;

/// 旧行・新行を同じ形の JSON で読む。driver_cd は employees を引いた値 (取り込みの
/// 乗務員解決は code 優先なので code も見る)。時刻は session の TimeZone に
/// 左右されないよう UTC で文字列にする。
const SNAPSHOT_SQL: &str = r#"
SELECT o.crew_role,
       jsonb_build_object(
           'driver_cd', COALESCE(e.driver_cd, e.code),
           'departure_at', to_char(o.departure_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"'),
           'return_at', to_char(o.return_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')
       )
  FROM alc_api.dtako_operations o
  LEFT JOIN alc_api.employees e ON e.id = o.driver_id
 WHERE o.tenant_id = $1 AND o.unko_no = $2 AND ($3::INTEGER IS NULL OR o.crew_role = $3)
 ORDER BY o.crew_role
"#;

/// `(crew_role, {driver_cd, departure_at, return_at})` を crew_role 順に返す。
/// `crew_role` が `None` なら運行の全 crew_role。
pub(crate) async fn fetch_snapshots(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    unko_no: &str,
    crew_role: Option<i32>,
) -> Result<Vec<(i32, serde_json::Value)>, sqlx::Error> {
    sqlx::query_as(SNAPSHOT_SQL)
        .bind(tenant_id)
        .bind(unko_no)
        .bind(crew_role)
        .fetch_all(conn)
        .await
}

/// 変更記録の 1 行。
pub(crate) struct NewChange<'a> {
    pub tenant_id: Uuid,
    pub unko_no: &'a str,
    pub crew_role: i32,
    pub upload_id: Option<Uuid>,
    pub reason: &'a str,
    pub before: Option<&'a serde_json::Value>,
    pub after: Option<&'a serde_json::Value>,
}

pub(crate) async fn insert_change(
    conn: &mut PgConnection,
    c: &NewChange<'_>,
) -> Result<(), sqlx::Error> {
    let driver_cd = crate::dtako_operation_changes::record_driver_cd(c.before, c.after);
    sqlx::query(
        r#"INSERT INTO alc_api.dtako_operation_changes
               (tenant_id, unko_no, crew_role, driver_cd, upload_id, reason, before, after)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
    )
    .bind(c.tenant_id)
    .bind(c.unko_no)
    .bind(c.crew_role)
    .bind(driver_cd)
    .bind(c.upload_id)
    .bind(c.reason)
    .bind(c.before)
    .bind(c.after)
    .execute(conn)
    .await?;
    Ok(())
}

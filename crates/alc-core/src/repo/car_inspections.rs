//! 電子車検証の番号で car_inspection から車検期限を引く (Refs ippoan/alc-app-s3#110)。
//!
//! 呼び元は 2 つ: 通常点呼の保存 (`alc-tenko` の normal_tenko — 測定と同じ
//! transaction に相乗り) と、kiosk の照合口 (`alc-carins` の repository —
//! `TenantConn` を取って呼ぶ)。どちらも RLS の効いた接続を渡す。
//! alc-tenko から alc-carins へ依存を張らないよう、ここ (alc-core) に置く。

use chrono::NaiveDate;
use sqlx::PgConnection;

pub use crate::repository::car_inspections::CarinsLookup;

/// 管理番号か車両 ID のどちらかで一致する行のうち、期限の最も新しい 1 行を引く。
///
/// * 継続検査で管理番号が変わった車でも、車両 ID で新しい行が引ける
///   (1 本の SQL で `OR` を取り、期限の新しい順に 1 行)
/// * `matched_by` はその行で判定する: 管理番号が一致すれば `cert_no`、
///   そうでなければ `car_id`。行が無ければ `none`
/// * 期限は YYMMDD (西暦下 2 桁)。月日の形を正規表現で検査してから `to_date` に
///   通し、外れた値は NULL にする
const LOOKUP_SQL: &str = r#"
SELECT
    COALESCE(ci."ElectCertMgNo" = $1::text, FALSE) AS by_cert_no,
    CASE
        WHEN ci."TwodimensionCodeInfoValidPeriodExpirdate"
             ~ '^\d{2}(0[1-9]|1[0-2])(0[1-9]|[12]\d|3[01])$'
        THEN to_date('20' || ci."TwodimensionCodeInfoValidPeriodExpirdate", 'YYYYMMDD')
    END AS expires_on,
    NULLIF(ci."EntryNoCarNo", '') AS car_no
FROM car_inspection ci
WHERE ci."ElectCertMgNo" = $1::text OR ci."CarId" = $2::text
ORDER BY expires_on DESC NULLS LAST, ci.created_at DESC
LIMIT 1
"#;

pub async fn lookup_expiry(
    conn: &mut PgConnection,
    cert_no: Option<&str>,
    car_id: Option<&str>,
) -> Result<CarinsLookup, sqlx::Error> {
    let row = sqlx::query_as::<_, (bool, Option<NaiveDate>, Option<String>)>(LOOKUP_SQL)
        .bind(cert_no)
        .bind(car_id)
        .fetch_optional(conn)
        .await?;
    Ok(match row {
        Some((by_cert_no, expires_on, car_no)) => CarinsLookup {
            expires_on,
            matched_by: if by_cert_no { "cert_no" } else { "car_id" },
            car_no,
        },
        None => CarinsLookup {
            expires_on: None,
            matched_by: "none",
            car_no: None,
        },
    })
}

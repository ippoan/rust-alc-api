//! 電子車検証の番号で car_inspection から車検期限を引く (Refs ippoan/alc-app-s3#110)。
//!
//! 呼び元は 2 つ: 通常点呼の保存 (`alc-tenko` の normal_tenko — 測定と同じ
//! transaction に相乗り) と、kiosk の照合口 (`alc-carins` の repository —
//! `TenantConn` を取って呼ぶ)。どちらも RLS の効いた接続を渡す。
//! alc-tenko から alc-carins へ依存を張らないよう、ここ (alc-core) に置く。

use chrono::NaiveDate;
use sqlx::PgConnection;

pub use crate::repository::car_inspections::CarinsLookup;

/// 元号 → 西暦のオフセット表 (元号の和暦年 + オフセット = 西暦年)。
///
/// `LOOKUP_SQL` の CASE に直書きせずここから bind する (Refs ippoan/alc-app-s3#110) —
/// 元号が増えたときに migration を打たずここへ 1 行足すだけで済む。表記ゆれ
/// (`令和` / `令 和` / `R` 等) は実データを見ずに決められないため吸収しない。
/// SQL 側で空白 (半角/全角) を除いた完全一致だけを見る。
const ERA_OFFSETS: &[(&str, i32)] = &[("令和", 2018), ("平成", 1988)];

/// 元号 (空白除去済み) + 和暦年から西暦年を返す。表に無い元号は None
/// (エラーにしない — `LOOKUP_SQL` 側の和暦 CASE と同じ方針)。
/// `LOOKUP_SQL` が行う変換をユニットテストで直接検査するための純関数。
pub fn era_to_western_year(era: &str, wareki_year: i32) -> Option<i32> {
    ERA_OFFSETS
        .iter()
        .find(|&&(name, _)| name == era)
        .map(|&(_, offset)| offset + wareki_year)
}

/// 管理番号か車両 ID のどちらかで一致する行のうち、期限の最も新しい 1 行を引く。
///
/// * 継続検査で管理番号が変わった車でも、車両 ID で新しい行が引ける
///   (1 本の SQL で `OR` を取り、期限の新しい順に 1 行)
/// * `matched_by` はその行で判定する: 管理番号が一致すれば `cert_no`、
///   そうでなければ `car_id`。行が無ければ `none`
/// * 期限は 2 通りから COALESCE で決める (Refs ippoan/alc-app-s3#110):
///   1. `TwodimensionCodeInfoValidPeriodExpirdate` (2 次元コード由来、YYMMDD)。
///      月日の形を正規表現で検査してから `to_date` に通し、外れた値は NULL
///   2. 1 が空文字列 (2 次元コードなしで保存された行) のときだけ、和暦 4 列
///      (`ValidPeriodExpirdateE`/`Y`/`M`/`D`) から組み立てる。元号は
///      `ERA_OFFSETS` を bind した表引き、年月日は数字かつ実在する日付の
///      ときだけ日付にし、外れた値・未知の元号は NULL (エラーにしない)
/// * ORDER BY はこの COALESCE 後の値に対して効く
const LOOKUP_SQL: &str = r#"
WITH era_map(era, offset_year) AS (
    SELECT * FROM unnest($3::text[], $4::int[])
)
SELECT
    COALESCE(ci."ElectCertMgNo" = $1::text, FALSE) AS by_cert_no,
    COALESCE(
        CASE
            WHEN ci."TwodimensionCodeInfoValidPeriodExpirdate"
                 ~ '^\d{2}(0[1-9]|1[0-2])(0[1-9]|[12]\d|3[01])$'
            THEN to_date('20' || ci."TwodimensionCodeInfoValidPeriodExpirdate", 'YYYYMMDD')
        END,
        wareki.expires_on
    ) AS expires_on,
    NULLIF(ci."EntryNoCarNo", '') AS car_no
FROM car_inspection ci
LEFT JOIN LATERAL (
    -- 和暦 4 列から期限を組み立てる。to_date / make_date が例外を投げないよう、
    -- 年月日はすべて事前に範囲検査してから最後に 1 回だけ make_date を呼ぶ
    SELECT fin.expires_on
    FROM (
        SELECT
            CASE WHEN ci."ValidPeriodExpirdateY" ~ '^\d{1,4}$'
                 THEN em.offset_year + ci."ValidPeriodExpirdateY"::int END AS y,
            CASE WHEN ci."ValidPeriodExpirdateM" ~ '^\d{1,2}$'
                 THEN ci."ValidPeriodExpirdateM"::int END AS m,
            CASE WHEN ci."ValidPeriodExpirdateD" ~ '^\d{1,2}$'
                 THEN ci."ValidPeriodExpirdateD"::int END AS d
        FROM era_map em
        WHERE em.era = regexp_replace(ci."ValidPeriodExpirdateE", '[[:space:]　]', '', 'g')
        LIMIT 1
    ) parsed
    CROSS JOIN LATERAL (
        SELECT CASE
            WHEN (parsed.y BETWEEN 1 AND 9999) AND (parsed.m BETWEEN 1 AND 12)
            THEN date_part(
                'day',
                make_date(parsed.y, parsed.m, 1) + interval '1 month - 1 day'
            )::int
        END AS last_day
    ) bnd
    CROSS JOIN LATERAL (
        SELECT CASE
            WHEN parsed.d BETWEEN 1 AND bnd.last_day
            THEN make_date(parsed.y, parsed.m, parsed.d)
        END AS expires_on
    ) fin
) wareki ON TRUE
WHERE ci."ElectCertMgNo" = $1::text OR ci."CarId" = $2::text
ORDER BY expires_on DESC NULLS LAST, ci.created_at DESC
LIMIT 1
"#;

pub async fn lookup_expiry(
    conn: &mut PgConnection,
    cert_no: Option<&str>,
    car_id: Option<&str>,
) -> Result<CarinsLookup, sqlx::Error> {
    let era_names: Vec<&str> = ERA_OFFSETS.iter().map(|&(name, _)| name).collect();
    let era_offsets: Vec<i32> = ERA_OFFSETS.iter().map(|&(_, offset)| offset).collect();
    let row = sqlx::query_as::<_, (bool, Option<NaiveDate>, Option<String>)>(LOOKUP_SQL)
        .bind(cert_no)
        .bind(car_id)
        .bind(&era_names)
        .bind(&era_offsets)
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::PgPool;
    use uuid::Uuid;

    /// 元号オフセットの純粋な変換ロジックをテストする (DB 不要)。
    #[test]
    fn era_to_western_year_known_eras() {
        assert_eq!(era_to_western_year("令和", 8), Some(2026));
        assert_eq!(era_to_western_year("平成", 31), Some(2019));
    }

    #[test]
    fn era_to_western_year_unknown_era_is_none() {
        assert_eq!(era_to_western_year("大正", 1), None);
        assert_eq!(era_to_western_year("", 1), None);
    }

    /// テスト専用コンテナ (alc-pg-110-9) へ接続し migration を適用したプールを返す。
    /// `docker-entrypoint-initdb.d` に `scripts/init_local_db.sql` を積んだ
    /// コンテナを前提 (alc_api スキーマ・ロールは初期化済み)。
    ///
    /// `TEST_DATABASE_URL` 未設定 (= `make test` / `cargo test --lib` の DB 不要な
    /// 実行) では None を返して呼び出し側で skip する — `--lib --workspace` は
    /// DB 不要が前提のため、ここで panic すると CI の unit-tests job を壊す。
    async fn test_pool() -> Option<PgPool> {
        let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
            eprintln!(
                "TEST_DATABASE_URL 未設定のため car_inspections の DB テストを skip \
                 (専用 postgres コンテナを起動し接続先を渡すと実行される)"
            );
            return None;
        };
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await
            .expect("テスト DB へ接続できない");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("migration の適用に失敗");
        // car_inspection は 90+ の NOT NULL TEXT 列を持つ。テストで触らない列を
        // 都度書ききるのを避けるため、テスト DB だけに一時的な DEFAULT '' を
        // 張る (migrations/*.sql は変更しない — テスト専用の後付け)
        sqlx::query(
            r#"
            DO $$
            DECLARE
                col RECORD;
            BEGIN
                FOR col IN
                    SELECT column_name
                    FROM information_schema.columns
                    WHERE table_schema = 'alc_api'
                      AND table_name = 'car_inspection'
                      AND data_type = 'text'
                      AND is_nullable = 'NO'
                      AND column_default IS NULL
                LOOP
                    EXECUTE format(
                        'ALTER TABLE alc_api.car_inspection ALTER COLUMN %I SET DEFAULT ''''',
                        col.column_name
                    );
                END LOOP;
            END $$;
            "#,
        )
        .execute(&pool)
        .await
        .expect("car_inspection の列 DEFAULT 設定に失敗");
        Some(pool)
    }

    /// car_inspection に合成行を 1 件入れる (実在の管理番号・車両 ID・登録番号は使わない)。
    #[allow(clippy::too_many_arguments)]
    async fn seed_car_inspection(
        pool: &PgPool,
        tenant_id: Uuid,
        cert_no: &str,
        car_no: &str,
        twod_expiry: &str,
        era: &str,
        wareki_y: &str,
        wareki_m: &str,
        wareki_d: &str,
    ) {
        sqlx::query(
            r#"
            INSERT INTO alc_api.car_inspection (
                tenant_id, "ElectCertMgNo", "CarId", "EntryNoCarNo",
                "TwodimensionCodeInfoValidPeriodExpirdate",
                "ValidPeriodExpirdateE", "ValidPeriodExpirdateY",
                "ValidPeriodExpirdateM", "ValidPeriodExpirdateD"
            ) VALUES ($1, $2, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(tenant_id)
        .bind(cert_no)
        .bind(car_no)
        .bind(twod_expiry)
        .bind(era)
        .bind(wareki_y)
        .bind(wareki_m)
        .bind(wareki_d)
        .execute(pool)
        .await
        .expect("car_inspection への合成行 INSERT に失敗");
    }

    /// 2D の列 (TwodimensionCodeInfoValidPeriodExpirdate) が正しい行 →
    /// その日付が返る (和暦列が空でも既存の振る舞いは変わらない)
    #[tokio::test]
    async fn lookup_expiry_uses_2d_column_when_valid() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let tenant_id = Uuid::new_v4();
        let cert_no = format!("t2d-{}", Uuid::new_v4().simple());
        seed_car_inspection(
            &pool,
            tenant_id,
            &cert_no,
            "TEST-CAR-1",
            "271231",
            "",
            "",
            "",
            "",
        )
        .await;

        let mut conn = pool.acquire().await.expect("接続取得に失敗");
        let result = lookup_expiry(&mut conn, Some(&cert_no), None)
            .await
            .expect("lookup_expiry がエラーを返した");

        assert_eq!(result.expires_on, NaiveDate::from_ymd_opt(2027, 12, 31));
        assert_eq!(result.matched_by, "cert_no");
        assert_eq!(result.car_no.as_deref(), Some("TEST-CAR-1"));
    }

    /// 2D の列が空文字列で、和暦 4 列が入っている行 →
    /// 和暦から組み立てた日付が返る
    #[tokio::test]
    async fn lookup_expiry_falls_back_to_wareki_when_2d_empty() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let tenant_id = Uuid::new_v4();
        let cert_no = format!("wareki-{}", Uuid::new_v4().simple());
        seed_car_inspection(
            &pool,
            tenant_id,
            &cert_no,
            "TEST-CAR-2",
            "",
            "令和",
            "8",
            "2",
            "13",
        )
        .await;

        let mut conn = pool.acquire().await.expect("接続取得に失敗");
        let result = lookup_expiry(&mut conn, Some(&cert_no), None)
            .await
            .expect("lookup_expiry がエラーを返した");

        assert_eq!(result.expires_on, NaiveDate::from_ymd_opt(2026, 2, 13));
        assert_eq!(result.matched_by, "cert_no");
    }

    /// 2D も和暦も壊れている行 → expires_on は None、エラーにはならず
    /// matched_by は当たったまま
    #[tokio::test]
    async fn lookup_expiry_returns_none_without_error_when_both_broken() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let tenant_id = Uuid::new_v4();
        let cert_no = format!("broken-{}", Uuid::new_v4().simple());
        // 2D 列は正規表現に外れる形、和暦は月が 13 (範囲外)
        seed_car_inspection(
            &pool,
            tenant_id,
            &cert_no,
            "TEST-CAR-3",
            "not-a-date",
            "令和",
            "8",
            "13",
            "01",
        )
        .await;

        let mut conn = pool.acquire().await.expect("接続取得に失敗");
        let result = lookup_expiry(&mut conn, Some(&cert_no), None)
            .await
            .expect("lookup_expiry がエラーを返した (壊れた値で例外になってはいけない)");

        assert_eq!(result.expires_on, None);
        assert_eq!(result.matched_by, "cert_no");
    }

    /// 和暦の元号が未知の値 → None (エラーにしない)
    #[tokio::test]
    async fn lookup_expiry_returns_none_for_unknown_era() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let tenant_id = Uuid::new_v4();
        let cert_no = format!("unknown-era-{}", Uuid::new_v4().simple());
        seed_car_inspection(
            &pool,
            tenant_id,
            &cert_no,
            "TEST-CAR-4",
            "",
            "大正",
            "1",
            "1",
            "1",
        )
        .await;

        let mut conn = pool.acquire().await.expect("接続取得に失敗");
        let result = lookup_expiry(&mut conn, Some(&cert_no), None)
            .await
            .expect("lookup_expiry がエラーを返した");

        assert_eq!(result.expires_on, None);
        assert_eq!(result.matched_by, "cert_no");
    }
}

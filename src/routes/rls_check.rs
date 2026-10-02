//! `GET /api/internal/rls-check` — テナントごとの行の出し分け (RLS) が、いま繋いでいる DB で
//! 効いているかを、固定の問い合わせ 5 つで確かめる内部用の口 (Refs ippoan/auth-worker#605)。
//!
//! migration を当てた後にオーナーが SQL エディタで手で見ていたもの (不変条件の検査 / 実行用ロールの
//! 属性 / 繋いでいるロール / 適用履歴) を 1 回の呼び出しで返す。`require_internal_jwt` の配下に置く。
//!
//! 守ること:
//! - **引数を取らない。** extractor は `State` だけ。SQL・表名・テナントを受ける入口を足さない。
//! - **読み取りだけ。** transaction は [`begin_read_only`] からだけ取り、最後は rollback する。
//! - **行のデータを返さない。** 返すのはカタログ由来のロール名・表名・関数名・真偽・件数だけ。
//!   `pg_stat_activity` から読む列は `usename` と件数だけ (接続元・問い合わせの本文は読まない)。
//! - **検査 SQL の写しを置かない。** 不変条件の検査は `alc_migrations::RLS_INVARIANTS_QUERY` を、
//!   状態は `alc_migrations::RLS_STATE_QUERY` をそのまま流す (加工・連結しない)。検査の題の一覧も
//!   `alc_migrations::RLS_INVARIANT_CHECKS` を使う。
//! - **`ok` は `invariants.checks` と `state` を見ない。** どちらも「何を検査し、DB が実際に
//!   どうだったか」を返った値で確かめるための材料で、合否は違反 0 件 + ロールの属性 + 適用履歴の一致。
//!   `state` は取れなければ null (主の結果を道連れにしない)。
//! - どの問い合わせも bind する値を持たない。SQL を `format!` で組み立てない。

use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;
use sqlx::{PgPool, Postgres, Transaction};

use alc_core::api_error::{internal_error_msg, ApiResult};

use crate::AppState;

/// backend の実行用ロール (alc-migrations 158)。
const RUNTIME_ROLE: &str = "alc_api_rt";

/// 適用履歴の件数と最大 version (alc-migrations 160 の SECURITY DEFINER 関数。
/// 実行用ロールは適用履歴の表を直接は読めない)。
const MIGRATION_STATUS_QUERY: &str =
    "SELECT applied, COALESCE(max_version, 0) FROM alc_api.migration_status()";

/// いまの接続のロールと、実行用ロールの属性。実行用ロールの行が無ければ 0 行 (= 500)。
/// 最後の列は、`alc_api` の表の所有者のどれかの資格を実行用ロールが取れるか
/// (所有者のロール名は直書きしない)。
const RUNTIME_ROLE_QUERY: &str = "\
SELECT current_user::text, r.rolsuper, r.rolbypassrls, r.rolinherit, \
       COALESCE(( \
           SELECT bool_or(pg_has_role(r.oid, c.relowner, 'MEMBER')) \
             FROM pg_class c \
            WHERE c.relnamespace = 'alc_api'::regnamespace \
              AND c.relkind IN ('r', 'p') \
       ), false) \
  FROM pg_roles r \
 WHERE r.rolname = 'alc_api_rt'";

/// この DB に繋いでいる `alc_api` 系のロールごとの接続数。読む列は `usename` と件数だけ。
const CONNECTIONS_QUERY: &str = r#"
SELECT a.usename::text, count(*)
  FROM pg_stat_activity a
 WHERE a.datname = current_database()
   AND a.usename LIKE 'alc\_api%'
 GROUP BY a.usename
 ORDER BY a.usename"#;

/// 上の接続の中に、`alc_api` の表の所有者であるロールのものが在るか。
const OWNER_ROLE_CONNECTED_QUERY: &str = r#"
SELECT EXISTS (
    SELECT 1
      FROM pg_stat_activity a
     WHERE a.datname = current_database()
       AND a.usename LIKE 'alc\_api%'
       AND a.usesysid IN (
           SELECT c.relowner
             FROM pg_class c
            WHERE c.relnamespace = 'alc_api'::regnamespace
              AND c.relkind IN ('r', 'p')
       )
)"#;

#[derive(Debug, Serialize)]
pub struct RlsCheckResponse {
    pub ok: bool,
    pub migrations: MigrationsStatus,
    pub runtime_role: RuntimeRoleStatus,
    pub connections: Vec<ConnectionCount>,
    pub owner_role_connected: bool,
    pub invariants: InvariantsStatus,
    /// `alc_api` schema のカタログの実物 (`alc_migrations::RLS_STATE_QUERY` の JSON をそのまま)。
    /// 取れなければ null。`ok` には入れない。
    pub state: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct MigrationsStatus {
    /// DB に適用済みの件数
    pub applied: i64,
    /// DB に適用済みの最大 version (1 件も無ければ 0)
    pub max_version: i64,
    /// この binary に埋め込まれた migration の件数
    pub binary_count: i64,
    /// この binary に埋め込まれた migration の最大 version
    pub binary_max_version: i64,
    pub matches_binary: bool,
}

#[derive(Debug, Serialize)]
pub struct RuntimeRoleStatus {
    pub current_user: String,
    pub is_runtime_role: bool,
    pub rolsuper: bool,
    pub rolbypassrls: bool,
    pub rolinherit: bool,
    pub member_of_table_owner: bool,
}

#[derive(Debug, Serialize)]
pub struct ConnectionCount {
    pub usename: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct InvariantsStatus {
    pub violation_count: i64,
    pub violations: Vec<InvariantViolation>,
    /// 流した検査の一覧と、検査ごとの違反の数 (違反が無くても毎回並ぶ)
    pub checks: Vec<InvariantCheck>,
}

#[derive(Debug, Serialize)]
pub struct InvariantCheck {
    pub check_no: i32,
    pub title: String,
    pub violations: i64,
}

#[derive(Debug, Serialize)]
pub struct InvariantViolation {
    pub check_no: i32,
    pub object: String,
    pub detail: String,
}

/// `require_internal_jwt` (aud=alc-api-internal) 配下に nest される internal ルート。
pub fn internal_router() -> Router<AppState> {
    Router::new().route("/internal/rls-check", get(rls_check))
}

/// 読み取り専用の transaction を返す。この口が transaction・接続を取るのはここだけ。
/// `SET TRANSACTION READ ONLY` は、transaction の最初の問い合わせより前に流す必要がある。
pub async fn begin_read_only(pool: &PgPool) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION READ ONLY")
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

/// 全体の合否。所有者ロールの接続の有無 (`owner_role_connected`) は入れない
/// (migration の job の実行中は所有者ロールが一時的に繋ぐ)。
fn is_ok(role: &RuntimeRoleStatus, violation_count: i64, matches_binary: bool) -> bool {
    violation_count == 0
        && role.is_runtime_role
        && !role.rolsuper
        && !role.rolbypassrls
        && !role.rolinherit
        && !role.member_of_table_owner
        && matches_binary
}

/// 検査の一覧に無い番号の違反に付ける題。
const UNLISTED_CHECK_TITLE: &str = "(一覧に無い検査)";

/// 検査の一覧 (`listed` = 番号と題) に、違反の数を番号ごとに数えて付ける。
/// 一覧に無い番号の違反は、番号の昇順で末尾に足す (数の合計 = 違反の行数)。
fn build_checks(listed: &[(i32, &str)], violations: &[InvariantViolation]) -> Vec<InvariantCheck> {
    let count = |check_no: i32| violations.iter().filter(|v| v.check_no == check_no).count() as i64;
    let mut checks: Vec<InvariantCheck> = listed
        .iter()
        .map(|&(check_no, title)| InvariantCheck {
            check_no,
            title: title.to_string(),
            violations: count(check_no),
        })
        .collect();

    let mut unlisted: Vec<i32> = violations
        .iter()
        .map(|v| v.check_no)
        .filter(|no| !listed.iter().any(|&(listed_no, _)| listed_no == *no))
        .collect();
    unlisted.sort_unstable();
    unlisted.dedup();
    checks.extend(unlisted.into_iter().map(|check_no| InvariantCheck {
        check_no,
        title: UNLISTED_CHECK_TITLE.to_string(),
        violations: count(check_no),
    }));
    checks
}

/// この binary に埋め込まれた migration の (件数, 最大 version)。
fn binary_migrations() -> (i64, i64) {
    let versions = alc_migrations::MIGRATOR
        .iter()
        .filter(|m| !m.migration_type.is_down_migration())
        .map(|m| m.version);
    versions.fold((0, 0), |(count, max), v| (count + 1, max.max(v)))
}

/// 固定の問い合わせ 5 つを流して応答を組み立てる (`state` は呼び出し元が後から入れる)。
async fn collect(tx: &mut Transaction<'static, Postgres>) -> Result<RlsCheckResponse, sqlx::Error> {
    let (applied, max_version): (i64, i64) = sqlx::query_as(MIGRATION_STATUS_QUERY)
        .fetch_one(&mut **tx)
        .await?;
    let (binary_count, binary_max_version) = binary_migrations();
    let matches_binary = applied == binary_count && max_version == binary_max_version;

    let (current_user, rolsuper, rolbypassrls, rolinherit, member_of_table_owner): (
        String,
        bool,
        bool,
        bool,
        bool,
    ) = sqlx::query_as(RUNTIME_ROLE_QUERY)
        .fetch_one(&mut **tx)
        .await?;
    let runtime_role = RuntimeRoleStatus {
        is_runtime_role: current_user == RUNTIME_ROLE,
        current_user,
        rolsuper,
        rolbypassrls,
        rolinherit,
        member_of_table_owner,
    };

    let connections = sqlx::query_as::<_, (String, i64)>(CONNECTIONS_QUERY)
        .fetch_all(&mut **tx)
        .await?
        .into_iter()
        .map(|(usename, count)| ConnectionCount { usename, count })
        .collect();

    let owner_role_connected: bool = sqlx::query_scalar(OWNER_ROLE_CONNECTED_QUERY)
        .fetch_one(&mut **tx)
        .await?;

    let violations: Vec<InvariantViolation> =
        sqlx::query_as::<_, (i32, String, String)>(alc_migrations::RLS_INVARIANTS_QUERY)
            .fetch_all(&mut **tx)
            .await?
            .into_iter()
            .map(|(check_no, object, detail)| InvariantViolation {
                check_no,
                object,
                detail,
            })
            .collect();
    let violation_count = violations.len() as i64;
    let checks = build_checks(alc_migrations::RLS_INVARIANT_CHECKS, &violations);

    Ok(RlsCheckResponse {
        ok: is_ok(&runtime_role, violation_count, matches_binary),
        migrations: MigrationsStatus {
            applied,
            max_version,
            binary_count,
            binary_max_version,
            matches_binary,
        },
        runtime_role,
        connections,
        owner_role_connected,
        invariants: InvariantsStatus {
            violation_count,
            violations,
            checks,
        },
        state: None,
    })
}

/// カタログの実物 (1 行 1 列の JSON)。取れなければ None (ログに 1 行)。
///
/// **transaction の最後に流す**: PostgreSQL は transaction の中で文が失敗すると以後の文が全部
/// 失敗するので、これより後に問い合わせを足さない。
async fn fetch_state(tx: &mut Transaction<'static, Postgres>) -> Option<serde_json::Value> {
    match sqlx::query_scalar::<_, serde_json::Value>(alc_migrations::RLS_STATE_QUERY)
        .fetch_one(&mut **tx)
        .await
    {
        Ok(state) => Some(state),
        Err(e) => {
            tracing::error!("rls_check: state query failed: {e}");
            None
        }
    }
}

async fn run(pool: &PgPool) -> Result<RlsCheckResponse, sqlx::Error> {
    let mut tx = begin_read_only(pool).await?;
    let mut response = collect(&mut tx).await?;
    response.state = fetch_state(&mut tx).await;
    // 読み取りだけなので、rollback の失敗 (状態の問い合わせで接続が切れた等) で主の結果を捨てない
    if let Err(e) = tx.rollback().await {
        tracing::error!("rls_check: rollback failed: {e}");
    }
    Ok(response)
}

async fn rls_check(State(state): State<AppState>) -> ApiResult<RlsCheckResponse> {
    match run(state.pool()).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            // 本文には固定の文言だけを載せる (SQL・カタログの中身を出さない)
            tracing::error!("rls_check: {e}");
            Err(internal_error_msg("rls check failed"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy_role() -> RuntimeRoleStatus {
        RuntimeRoleStatus {
            current_user: RUNTIME_ROLE.to_string(),
            is_runtime_role: true,
            rolsuper: false,
            rolbypassrls: false,
            rolinherit: false,
            member_of_table_owner: false,
        }
    }

    fn keys(value: &serde_json::Value) -> Vec<String> {
        let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    }

    #[test]
    fn ok_when_everything_holds() {
        assert!(is_ok(&healthy_role(), 0, true));
    }

    #[test]
    fn not_ok_with_violations() {
        assert!(!is_ok(&healthy_role(), 1, true));
    }

    #[test]
    fn not_ok_when_not_runtime_role() {
        let role = RuntimeRoleStatus {
            current_user: "alc_api_app".to_string(),
            is_runtime_role: false,
            ..healthy_role()
        };
        assert!(!is_ok(&role, 0, true));
    }

    #[test]
    fn not_ok_when_any_role_attribute_is_true() {
        let superuser = RuntimeRoleStatus {
            rolsuper: true,
            ..healthy_role()
        };
        let bypassrls = RuntimeRoleStatus {
            rolbypassrls: true,
            ..healthy_role()
        };
        let inherit = RuntimeRoleStatus {
            rolinherit: true,
            ..healthy_role()
        };
        let owner_member = RuntimeRoleStatus {
            member_of_table_owner: true,
            ..healthy_role()
        };
        for role in [superuser, bypassrls, inherit, owner_member] {
            assert!(!is_ok(&role, 0, true), "{role:?}");
        }
    }

    #[test]
    fn not_ok_when_binary_does_not_match() {
        assert!(!is_ok(&healthy_role(), 0, false));
    }

    /// 所有者ロールが繋いでいても ok は変わらない (`is_ok` は `owner_role_connected` を受けない)。
    #[test]
    fn owner_role_connected_does_not_affect_ok() {
        let role = healthy_role();
        let response = RlsCheckResponse {
            ok: is_ok(&role, 0, true),
            migrations: MigrationsStatus {
                applied: 1,
                max_version: 1,
                binary_count: 1,
                binary_max_version: 1,
                matches_binary: true,
            },
            runtime_role: role,
            connections: vec![ConnectionCount {
                usename: "alc_api_owner".to_string(),
                count: 1,
            }],
            owner_role_connected: true,
            invariants: InvariantsStatus {
                violation_count: 0,
                violations: vec![],
                checks: vec![],
            },
            state: None,
        };
        assert!(response.owner_role_connected);
        assert!(response.ok);
    }

    /// 応答の key は契約ちょうど (auth-worker の tool がこの形を読む)。
    #[test]
    fn response_keys_match_the_contract() {
        let response = RlsCheckResponse {
            ok: false,
            migrations: MigrationsStatus {
                applied: 2,
                max_version: 3,
                binary_count: 2,
                binary_max_version: 3,
                matches_binary: true,
            },
            runtime_role: healthy_role(),
            connections: vec![ConnectionCount {
                usename: RUNTIME_ROLE.to_string(),
                count: 3,
            }],
            owner_role_connected: false,
            invariants: InvariantsStatus {
                violation_count: 1,
                violations: vec![InvariantViolation {
                    check_no: 1,
                    object: "table x".to_string(),
                    detail: "d".to_string(),
                }],
                checks: vec![InvariantCheck {
                    check_no: 1,
                    title: "t".to_string(),
                    violations: 1,
                }],
            },
            state: Some(serde_json::json!({ "table_count": 0 })),
        };
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(
            keys(&json),
            [
                "connections",
                "invariants",
                "migrations",
                "ok",
                "owner_role_connected",
                "runtime_role",
                "state"
            ]
        );
        assert_eq!(
            keys(&json["migrations"]),
            [
                "applied",
                "binary_count",
                "binary_max_version",
                "matches_binary",
                "max_version"
            ]
        );
        assert_eq!(
            keys(&json["runtime_role"]),
            [
                "current_user",
                "is_runtime_role",
                "member_of_table_owner",
                "rolbypassrls",
                "rolinherit",
                "rolsuper"
            ]
        );
        assert_eq!(keys(&json["connections"][0]), ["count", "usename"]);
        assert_eq!(
            keys(&json["invariants"]),
            ["checks", "violation_count", "violations"]
        );
        assert_eq!(
            keys(&json["invariants"]["checks"][0]),
            ["check_no", "title", "violations"]
        );
        assert!(json["invariants"]["checks"][0]["violations"].is_i64());
        assert!(json["state"].is_object());
        assert_eq!(
            keys(&json["invariants"]["violations"][0]),
            ["check_no", "detail", "object"]
        );
        assert!(json["migrations"]["applied"].is_i64());
        assert!(json["connections"][0]["count"].is_i64());
        assert!(json["invariants"]["violations"][0]["check_no"].is_i64());
    }

    fn violation(check_no: i32) -> InvariantViolation {
        InvariantViolation {
            check_no,
            object: format!("object {check_no}"),
            detail: "d".to_string(),
        }
    }

    fn numbers_and_counts(checks: &[InvariantCheck]) -> Vec<(i32, i64)> {
        checks.iter().map(|c| (c.check_no, c.violations)).collect()
    }

    /// 違反が無くても、一覧の検査が全部 (本物の一覧なら 6 つ) 0 件で並ぶ
    #[test]
    fn checks_list_every_check_without_violations() {
        let checks = build_checks(alc_migrations::RLS_INVARIANT_CHECKS, &[]);
        assert_eq!(
            numbers_and_counts(&checks),
            [(0, 0), (1, 0), (2, 0), (3, 0), (4, 0), (5, 0)]
        );
        for (check, (_, title)) in checks.iter().zip(alc_migrations::RLS_INVARIANT_CHECKS) {
            assert_eq!(check.title, *title);
        }
    }

    /// 違反は該当の番号にだけ数が立つ
    #[test]
    fn checks_count_violations_per_check() {
        let violations = [violation(5), violation(1), violation(5), violation(5)];
        let checks = build_checks(alc_migrations::RLS_INVARIANT_CHECKS, &violations);
        assert_eq!(
            numbers_and_counts(&checks),
            [(0, 0), (1, 1), (2, 0), (3, 0), (4, 0), (5, 3)]
        );
    }

    /// 一覧に無い番号の違反は、番号の昇順で末尾に足され、合計は違反の行数と合う
    #[test]
    fn checks_append_unlisted_numbers_and_sum_to_violation_count() {
        let listed = [(0, "zero"), (1, "one")];
        let violations = [
            violation(9),
            violation(1),
            violation(7),
            violation(9),
            violation(-1),
        ];
        let checks = build_checks(&listed, &violations);
        assert_eq!(
            numbers_and_counts(&checks),
            [(0, 0), (1, 1), (-1, 1), (7, 1), (9, 2)]
        );
        let titles: Vec<&str> = checks.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "zero",
                "one",
                UNLISTED_CHECK_TITLE,
                UNLISTED_CHECK_TITLE,
                UNLISTED_CHECK_TITLE
            ]
        );
        let total: i64 = checks.iter().map(|c| c.violations).sum();
        assert_eq!(total, violations.len() as i64);
    }

    /// 埋め込みの migration は 1 件以上で、最大 version は件数以上 (欠番が在るので等しいとは限らない)。
    #[test]
    fn binary_migrations_are_embedded() {
        let (count, max_version) = binary_migrations();
        assert!(count > 0);
        assert!(max_version >= count);
    }
}

//! LINE WORKS ログイン状況確認 (`GET /notify/lineworks/login-activity?days=N`, Refs #540)。
//!
//! LINE WORKS のトークは既読/未読を取得できないため、「しばらく LINE WORKS を見ていない人」を
//! 把握する手段として監査ログ API から最終アクティブ日時を出す。用途は確認 (一覧表示) のみで、
//! リマインド送信等の自動アクションは行わない。
//!
//! ## 3つの信号を合成する (2026-09-11、ユーザーとの調査で判明した制約への対応)
//! `service=auth` (明示ログイン) 単体では、モバイルアプリがセッションを保持する
//! ユーザー (トラック乗務員のような「受け取るだけ」の利用が主なメンバー) の実態を
//! ほぼ反映できない (本番実測: 187人中167人が「記録なし」)。次の3信号を合成して
//! 「分かっている最も新しい下限値」を最終アクティブ日時として扱う:
//!
//! 1. **`service=auth`** — 明示的なログイン日時 (最も確実、これだけは必須)。
//! 2. **`service=message`** — トーク送信日時 (`parse_login_audit_csv` を流用。
//!    列構成が auth と同じ前提で「結果」列相当が見つからなければ全行を有効な
//!    送信イベントとして扱う設計になっている — メッセージ送信に成功/失敗の概念は
//!    無いため都合が良い)。**送信した履歴の無い「読むだけ」のメンバーには効かない**
//!    (LINE WORKS のトーク監査ログには受信・既読の記録が無く、送信の一覧しか
//!    取れないとコミュニティで報告されている)。
//! 3. **`service=board.read` (掲示板既読)** — 掲示板投稿の既読フラグからの**下限値**。
//!    既読 API (`/v1.0/boards/{id}/posts/{id}/readers`) は既読日時を返さず
//!    true/false のみなので、「この投稿を読んだ = 投稿作成日時以降にアクティブ
//!    だったはず」という下限として扱う (実際の最終アクティブ日時はこれより新しい
//!    可能性がある)。board API 3種 (`list_boards` / `list_recent_board_posts` /
//!    `list_board_post_readers`) のレスポンス shape は 2026-09-11 に本番で実測済み。
//!    ただし投稿は月〜四半期に 1 回の頻度のため、30 日窓ではこの下限値は通常空になる
//!    (best-effort であり不具合ではない。各段の件数は info/warn ログに出る)。構造が
//!    ずれても 0 件になるだけで既存の auth/message 結果は壊さない設計
//!    (`fetch_board_activity_lower_bound` の doc 参照)。
//!
//! 各行は合成値 `last_login_at` と一緒に、その値を出した信号 `last_login_source`
//! (`"auth"` / `"message"` / `"board"`、記録なしは `null`) を返す (2026-09-11、#540
//! フォローアップ)。画面 (`/admin/notify` のログイン状況タブ) が「この日時はログインか
//! メッセージ送信か掲示板既読か」を表示し絞り込むため。同時刻なら auth > message > board
//! の順で決める (`activity_source` 参照)。信号単体の日時は返さない — source が一致する
//! 信号の値は `last_login_at` と同じになり、情報が重複するため。
//!
//! **2 と 3 は best-effort**: scope 未設定・API 未対応・構造不一致などで失敗しても
//! ログに警告を出すだけで無視し、1 (auth) の結果はそのまま返す。1 の失敗だけが
//! エンドポイント全体のエラーになる。
//!
//! ## 既知の制約
//! - LINE WORKS の監査ログ CSV は列構成が公式ドキュメントに記載されていないが、
//!   2026-09-11 に本番テナントの実データで確認済み: `説明,メンバー,日時,IPアドレス,
//!   ログイン方法,サービスタイプ` の6列で、「メンバー」は `"表示名 (email@example.com)"`
//!   形式。詳細は `clients::lineworks::parse_login_audit_csv` の doc 参照。
//! - 監査ログ API は「並行して呼び出さないでください」と明記されている。本 handler は
//!   プロセスローカルな mutex + 短命キャッシュ (`CACHE_TTL`) で直列化・再取得抑制するが、
//!   複数インスタンス構成では効かない (このコードベースに分散ロック機構が無いため。他の
//!   LINE WORKS クライアント呼び出しも同様にプロセスローカルなトークンキャッシュしか
//!   持たない — 詳細はコードレビュー時の調査メモ参照)。
//! - 1 リクエストの取得期間は API 上限 31 日ぎりぎりの `Duration::days(31)` を
//!   `now - N` 〜 `now` (ちょうど31*24h) で指定すると LINE WORKS 側が
//!   `400 LIMIT_EXCEEDED "Period must be within 31 days."` を返す (2026-09-11 本番実測、
//!   Refs #540)。開始・終了の暦日をまたぐと「31日を超える」と判定されるらしく、
//!   境界値ぴったりは弾かれる。★ さらに **監査ログダウンロード URL (302 → Location) は
//!   Location 先にも同じ `Authorization` を付け直す必要がある** — 詳細・修正は
//!   `clients::lineworks::LineworksBotClient::fetch_login_audit_csv` の doc 参照
//!   (2026-09-11 本番実測、audit.read scope 自体は正しく有効化されていたにも関わらず
//!   `401 Authentication failed` になっていた)。**`AUDIT_WINDOW_DAYS` は上限の 31 ではなく 30 に
//!   落として安全側に倒す。** `days` はその窓の中から「N 日以上ログインなし」を
//!   判定するしきい値であり、`AUDIT_WINDOW_DAYS` を超える `days` を渡しても
//!   それより前の記録は判定できない (「記録なし」表示になる)。

use std::collections::HashMap;
use std::sync::OnceLock;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Extension, Json, Router,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;

use alc_core::auth_middleware::TenantId;
use alc_core::AppState;

use crate::clients::lineworks::{
    parse_login_audit_csv, LastLoginByEmail, LineworksBotClient, LineworksBotConfig,
    LineworksBotError,
};
use crate::lineworks_config::resolve_lineworks_config;

pub fn tenant_router() -> Router<AppState> {
    Router::new().route(
        "/notify/lineworks/login-activity",
        axum::routing::get(get_login_activity),
    )
}

/// LINE WORKS API 上限は「1 リクエスト最長31日」だが、境界値ぴったり (31*24h) を
/// 指定すると暦日ベースの判定で弾かれる (2026-09-11 実測、上のモジュール doc 参照)。
/// 安全マージンを取って 30 日にする。
const AUDIT_WINDOW_DAYS: i64 = 30;
/// 「並行して呼び出さないでください」への対応 + 過度な再取得の抑制。
const CACHE_TTL_SECS: i64 = 300;

#[derive(Debug, Deserialize)]
struct LoginActivityQuery {
    #[serde(default = "default_days")]
    days: i64,
}
fn default_days() -> i64 {
    3
}

#[derive(Debug, Clone, Serialize)]
pub struct LoginActivityEntry {
    pub user_id: String,
    pub user_name: Option<String>,
    pub email: Option<String>,
    /// RFC3339。記録なしなら `null`。
    pub last_login_at: Option<String>,
    /// 記録なしなら `null`。
    pub days_since_login: Option<i64>,
    /// `true` = 記録なし、または `days` しきい値以上ログインが無い。
    pub stale: bool,
    /// `last_login_at` を出した信号。`"auth"` (ログイン監査ログ) / `"message"` (トーク送信) /
    /// `"board"` (掲示板既読の下限値)。記録なしなら `null`。値は auth-worker の
    /// `/admin/notify` の絞り込み (`la-source` の option value) と一致させること。
    pub last_login_source: Option<&'static str>,
}

/// auth/message/board の3信号を合成した結果。`combined` が「分かっている最も新しい
/// 下限」(last_login_at / stale 判定の実体)。`auth` / `message` / `board` は各信号単体の
/// 値の内訳で、`combined` の値をどの信号が出したか (`last_login_source`) を
/// `activity_source` が決めるのに使う。
#[derive(Debug, Clone)]
struct CombinedActivity {
    combined: LastLoginByEmail,
    auth: LastLoginByEmail,
    message: LastLoginByEmail,
    board: LastLoginByEmail,
}

struct CachedActivity {
    fetched_at: DateTime<Utc>,
    activity: CombinedActivity,
}

type ActivityCache = Mutex<HashMap<Uuid, CachedActivity>>;

fn cache() -> &'static ActivityCache {
    static CACHE: OnceLock<ActivityCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn get_login_activity(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantId>,
    Query(query): Query<LoginActivityQuery>,
) -> Result<Json<Vec<LoginActivityEntry>>, (StatusCode, Json<serde_json::Value>)> {
    let days = query.days.max(0);
    let (config_id, config) = resolve_lineworks_config(&state, tenant.0).await?;
    let client = LineworksBotClient::new();

    let members = client
        .list_org_users(config_id, &config)
        .await
        .map_err(|e| scope_error(e, "directory.read"))?;

    let activity =
        get_or_fetch_login_activity(&client, config_id, &config, tenant.0, &members).await?;

    let now = Utc::now();
    let mut entries: Vec<LoginActivityEntry> = members
        .into_iter()
        .map(|m| {
            let email_lower = m.email.as_ref().map(|e| e.to_lowercase());
            let last_login = email_lower
                .as_ref()
                .and_then(|e| activity.combined.get(e).copied());
            let last_login_source = email_lower
                .as_deref()
                .and_then(|e| activity_source(&activity, e));
            let days_since_login = last_login.map(|dt| (now - dt).num_days());
            let stale = days_since_login.map(|d| d >= days).unwrap_or(true);
            LoginActivityEntry {
                user_id: m.user_id,
                user_name: m.user_name,
                email: m.email,
                last_login_at: last_login.map(|dt| dt.to_rfc3339()),
                days_since_login,
                stale,
                last_login_source,
            }
        })
        .collect();

    // 「記録なし」を先頭に、その次はログインが古い順 (確認したい対象が上に来るように)。
    entries.sort_by(|a, b| match (a.days_since_login, b.days_since_login) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (Some(x), Some(y)) => y.cmp(&x),
    });

    Ok(Json(entries))
}

/// tenant ごとに直列化 + `CACHE_TTL_SECS` 秒キャッシュしてから監査ログを取得する。
/// ロックを保持したまま fetch することで、同一 tenant への並行アクセスを直列化する
/// (「並行して呼び出さないでください」という LINE WORKS API 制約への対応)。
///
/// auth (必須) + message・board (best-effort) を合成する。後者2つが失敗しても
/// auth の結果はそのまま返す (モジュール doc 参照)。
async fn get_or_fetch_login_activity(
    client: &LineworksBotClient,
    config_id: Uuid,
    config: &LineworksBotConfig,
    tenant_id: Uuid,
    members: &[crate::clients::lineworks::LineworksMember],
) -> Result<CombinedActivity, (StatusCode, Json<serde_json::Value>)> {
    let mut guard = cache().lock().await;

    if let Some(cached) = guard.get(&tenant_id) {
        if Utc::now() - cached.fetched_at < Duration::seconds(CACHE_TTL_SECS) {
            return Ok(cached.activity.clone());
        }
    }

    let now = Utc::now();
    let start = now - Duration::days(AUDIT_WINDOW_DAYS);

    // 1. auth (必須)。失敗はそのままエンドポイントのエラーにする。
    let auth_csv = client
        .fetch_audit_csv(config_id, config, "auth", start, now)
        .await
        .map_err(|e| scope_error(e, "audit.read"))?;
    let auth_by_email = parse_login_audit_csv(&auth_csv).map_err(|e| {
        tracing::error!("parse login audit csv (auth): {e}");
        (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "error": "csv_columns_not_recognized",
                "message": e,
            })),
        )
    })?;

    // 2. message (best-effort)。トーク送信日時。失敗してもログに残すだけで進む。
    let message_by_email: LastLoginByEmail = match client
        .fetch_audit_csv(config_id, config, "message", start, now)
        .await
    {
        Ok(csv) => match parse_login_audit_csv(&csv) {
            Ok(sent_by_email) => sent_by_email,
            Err(e) => {
                tracing::warn!("parse message audit csv (best-effort, skip): {e}");
                HashMap::new()
            }
        },
        Err(e) => {
            tracing::warn!("fetch message audit csv (best-effort, skip): {e}");
            HashMap::new()
        }
    };

    // 3. board (best-effort)。既読投稿の作成日時を「下限」として合成する。
    let board_by_email: LastLoginByEmail = match client
        .fetch_board_activity_lower_bound(config_id, config, start)
        .await
    {
        Ok(lower_bound_by_user_id) => {
            let email_by_user_id: HashMap<&str, &str> = members
                .iter()
                .filter_map(|m| m.email.as_deref().map(|e| (m.user_id.as_str(), e)))
                .collect();
            let readers = lower_bound_by_user_id.len();
            let mut by_email: LastLoginByEmail = HashMap::new();
            for (user_id, dt) in lower_bound_by_user_id {
                if let Some(&email) = email_by_user_id.get(user_id.as_str()) {
                    by_email.insert(email.to_lowercase(), dt);
                }
            }
            // 突合で落ちた数を残す。userId が一致しない既読者・email の無いメンバーは
            // ここで黙って捨てるので、件数が無いと 0 件の原因を切り分けられない (Refs #540)。
            tracing::info!(
                "board activity: readers={readers} matched_to_member_email={}",
                by_email.len()
            );
            by_email
        }
        Err(e) => {
            tracing::warn!("fetch board activity (best-effort, skip): {e}");
            HashMap::new()
        }
    };

    let activity = combine_activity(auth_by_email, message_by_email, board_by_email);

    guard.insert(
        tenant_id,
        CachedActivity {
            fetched_at: now,
            activity: activity.clone(),
        },
    );

    Ok(activity)
}

/// auth/message/board を「一番新しい下限」に合成しつつ、信号別の内訳も保持する。
/// ネットワーク呼び出しを含まない純粋関数にしてあるのは、3信号がそれぞれ独立して
/// 保持されること (どれか1つに潰れないこと) を単体テストで確認できるようにするため。
fn combine_activity(
    auth: LastLoginByEmail,
    message: LastLoginByEmail,
    board: LastLoginByEmail,
) -> CombinedActivity {
    let mut combined = auth.clone();
    merge_keep_latest(&mut combined, message.clone());
    merge_keep_latest(&mut combined, board.clone());
    CombinedActivity {
        combined,
        auth,
        message,
        board,
    }
}

/// `combined` の値 (= `last_login_at`) をどの信号が出したかを返す。合成値と**等しい**
/// 信号を auth → message → board の順で探すので、同時刻なら auth > message > board。
/// 合成値が無ければ (記録なし) `None`。`email` は小文字化済みのキー。
fn activity_source(activity: &CombinedActivity, email: &str) -> Option<&'static str> {
    let latest = activity.combined.get(email)?;
    [
        ("auth", &activity.auth),
        ("message", &activity.message),
        ("board", &activity.board),
    ]
    .into_iter()
    .find(|(_, signal)| signal.get(email) == Some(latest))
    .map(|(source, _)| source)
}

/// `other` の各エントリを `base` にマージし、同じキーがあれば新しい方の日時を残す。
fn merge_keep_latest(base: &mut LastLoginByEmail, other: LastLoginByEmail) {
    for (email, dt) in other {
        base.entry(email)
            .and_modify(|cur| {
                if dt > *cur {
                    *cur = dt;
                }
            })
            .or_insert(dt);
    }
}

fn scope_error(e: LineworksBotError, scope: &str) -> (StatusCode, Json<serde_json::Value>) {
    let msg = e.to_string();
    tracing::error!("LINE WORKS login-activity ({scope}): {msg}");
    // 403 in the upstream response often maps to SendFailed("403: ...") (lineworks_directory.rs と同方針)。
    if msg.contains("403") || msg.to_lowercase().contains("forbidden") {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "missing_scope",
                "scope": scope,
                "message": format!(
                    "LINE WORKS Developer Console で {scope} scope を追加してください"
                ),
            })),
        );
    }
    (
        StatusCode::BAD_GATEWAY,
        Json(serde_json::json!({
            "error": "upstream_error",
            "message": msg,
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::assertions_on_constants)] // 定数への回帰ガードとして意図的
    fn audit_window_stays_under_the_31_day_api_limit() {
        // 2026-09-11 本番実測: ちょうど 31*24h を指定すると LINE WORKS 側が
        // `LIMIT_EXCEEDED "Period must be within 31 days."` を返す (暦日ベースの
        // 判定で境界値ぴったりは弾かれる)。安全マージンを削って 31 に戻す変更を
        // 防ぐための回帰テスト。
        assert!(
            AUDIT_WINDOW_DAYS < 31,
            "31 ちょうどは LINE WORKS 側に拒否される"
        );
    }

    #[test]
    fn merge_keep_latest_prefers_newer_entry_for_shared_key() {
        let mut base: LastLoginByEmail = HashMap::new();
        base.insert("a@x.com".to_string(), Utc::now() - Duration::days(10));

        let mut other: LastLoginByEmail = HashMap::new();
        other.insert("a@x.com".to_string(), Utc::now() - Duration::days(2)); // より新しい
        other.insert("b@x.com".to_string(), Utc::now() - Duration::days(5)); // 新規キー

        let a_before = *base.get("a@x.com").unwrap();
        merge_keep_latest(&mut base, other.clone());

        assert!(
            base.get("a@x.com").unwrap() > &a_before,
            "新しい方で上書きされる"
        );
        assert_eq!(
            base.get("b@x.com"),
            other.get("b@x.com"),
            "無いキーは追加される"
        );
    }

    #[test]
    fn combine_activity_keeps_per_signal_breakdown_and_merges_combined() {
        let mut auth: LastLoginByEmail = HashMap::new();
        auth.insert("a@x.com".to_string(), Utc::now() - Duration::days(10));

        let mut message: LastLoginByEmail = HashMap::new();
        message.insert("a@x.com".to_string(), Utc::now() - Duration::days(5)); // authより新しい
        message.insert("b@x.com".to_string(), Utc::now() - Duration::days(3));

        let mut board: LastLoginByEmail = HashMap::new();
        board.insert("a@x.com".to_string(), Utc::now() - Duration::days(20)); // 他の信号より古い
        board.insert("c@x.com".to_string(), Utc::now() - Duration::days(1)); // auth/messageに無いキー

        let activity = combine_activity(auth.clone(), message.clone(), board.clone());

        // combined は各キーごとに3信号のうち最新を採用する。
        assert_eq!(activity.combined.get("a@x.com"), message.get("a@x.com"));
        assert_eq!(activity.combined.get("b@x.com"), message.get("b@x.com"));
        assert_eq!(activity.combined.get("c@x.com"), board.get("c@x.com"));

        // 信号別の内訳は combined に潰されず、入力そのままで残る
        // (last_login_source が combined の値の出どころを判定できる根拠)。
        assert_eq!(activity.auth, auth);
        assert_eq!(activity.message, message);
        assert_eq!(activity.board, board);
        assert_eq!(activity_source(&activity, "a@x.com"), Some("message"));
        assert_eq!(activity_source(&activity, "b@x.com"), Some("message"));
        assert_eq!(activity_source(&activity, "c@x.com"), Some("board"));
    }

    fn at(days_ago: i64) -> DateTime<Utc> {
        // 同時刻の比較をするので Utc::now() ではなく固定の基準時刻から引く。
        DateTime::parse_from_rfc3339("2026-09-11T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
            - Duration::days(days_ago)
    }

    fn one(email: &str, dt: DateTime<Utc>) -> LastLoginByEmail {
        HashMap::from([(email.to_string(), dt)])
    }

    #[test]
    fn activity_source_auth_only_is_auth() {
        let activity = combine_activity(
            one("demo@example.com", at(3)),
            HashMap::new(),
            HashMap::new(),
        );
        assert_eq!(activity_source(&activity, "demo@example.com"), Some("auth"));
    }

    #[test]
    fn activity_source_message_newer_than_auth_is_message() {
        let activity = combine_activity(
            one("demo@example.com", at(10)),
            one("demo@example.com", at(2)),
            HashMap::new(),
        );
        assert_eq!(
            activity_source(&activity, "demo@example.com"),
            Some("message")
        );
    }

    #[test]
    fn activity_source_board_newest_is_board() {
        let activity = combine_activity(
            one("demo@example.com", at(10)),
            one("demo@example.com", at(5)),
            one("demo@example.com", at(1)),
        );
        assert_eq!(
            activity_source(&activity, "demo@example.com"),
            Some("board")
        );
    }

    #[test]
    fn activity_source_without_any_signal_is_none() {
        let activity = combine_activity(
            one("other@example.com", at(1)),
            HashMap::new(),
            HashMap::new(),
        );
        assert_eq!(activity_source(&activity, "demo@example.com"), None);
    }

    #[test]
    fn activity_source_same_time_prefers_auth_over_message() {
        let activity = combine_activity(
            one("demo@example.com", at(4)),
            one("demo@example.com", at(4)),
            HashMap::new(),
        );
        assert_eq!(activity_source(&activity, "demo@example.com"), Some("auth"));
    }

    #[test]
    fn merge_keep_latest_does_not_overwrite_with_older_entry() {
        let mut base: LastLoginByEmail = HashMap::new();
        let newer = Utc::now() - Duration::days(1);
        base.insert("a@x.com".to_string(), newer);

        let mut other: LastLoginByEmail = HashMap::new();
        other.insert("a@x.com".to_string(), Utc::now() - Duration::days(20)); // より古い

        merge_keep_latest(&mut base, other);

        assert_eq!(
            base.get("a@x.com"),
            Some(&newer),
            "古い方には上書きされない"
        );
    }
}

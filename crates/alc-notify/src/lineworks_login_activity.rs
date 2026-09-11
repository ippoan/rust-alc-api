//! LINE WORKS ログイン状況確認 (`GET /notify/lineworks/login-activity?days=N`, Refs #540)。
//!
//! LINE WORKS のトークは既読/未読を取得できないため、「しばらく LINE WORKS を見ていない人」を
//! 把握する手段として監査ログ API のログイン履歴から最終ログイン日時を出す。用途は確認
//! (一覧表示) のみで、リマインド送信等の自動アクションは行わない。
//!
//! ## 既知の制約 (要 実データ検証、issue #540 の TODO)
//! - LINE WORKS の監査ログ CSV は列構成が公式ドキュメントに記載されておらず、実サンプルも
//!   未取得。`clients::lineworks::parse_login_audit_csv` はキーワードによる推測パースなので、
//!   実データ入手後に見直すこと。
//! - 監査ログ API は「並行して呼び出さないでください」と明記されている。本 handler は
//!   プロセスローカルな mutex + 短命キャッシュ (`CACHE_TTL`) で直列化・再取得抑制するが、
//!   複数インスタンス構成では効かない (このコードベースに分散ロック機構が無いため。他の
//!   LINE WORKS クライアント呼び出しも同様にプロセスローカルなトークンキャッシュしか
//!   持たない — 詳細はコードレビュー時の調査メモ参照)。
//! - 1 リクエストの取得期間は API 上限 31 日ぎりぎりの `Duration::days(31)` を
//!   `now - N` 〜 `now` (ちょうど31*24h) で指定すると LINE WORKS 側が
//!   `400 LIMIT_EXCEEDED "Period must be within 31 days."` を返す (2026-09-11 本番実測、
//!   Refs #540)。開始・終了の暦日をまたぐと「31日を超える」と判定されるらしく、
//!   境界値ぴったりは弾かれる。**`AUDIT_WINDOW_DAYS` は上限の 31 ではなく 30 に
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
    parse_login_audit_csv, LastLoginByEmail, LineworksBotClient, LineworksBotError,
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
}

struct CachedActivity {
    fetched_at: DateTime<Utc>,
    last_login_by_email: LastLoginByEmail,
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

    let last_login_by_email =
        get_or_fetch_login_activity(&client, config_id, &config, tenant.0).await?;

    let now = Utc::now();
    let mut entries: Vec<LoginActivityEntry> = members
        .into_iter()
        .map(|m| {
            let last_login = m
                .email
                .as_ref()
                .and_then(|e| last_login_by_email.get(&e.to_lowercase()).copied());
            let days_since_login = last_login.map(|dt| (now - dt).num_days());
            let stale = days_since_login.map(|d| d >= days).unwrap_or(true);
            LoginActivityEntry {
                user_id: m.user_id,
                user_name: m.user_name,
                email: m.email,
                last_login_at: last_login.map(|dt| dt.to_rfc3339()),
                days_since_login,
                stale,
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
async fn get_or_fetch_login_activity(
    client: &LineworksBotClient,
    config_id: Uuid,
    config: &crate::clients::lineworks::LineworksBotConfig,
    tenant_id: Uuid,
) -> Result<LastLoginByEmail, (StatusCode, Json<serde_json::Value>)> {
    let mut guard = cache().lock().await;

    if let Some(cached) = guard.get(&tenant_id) {
        if Utc::now() - cached.fetched_at < Duration::seconds(CACHE_TTL_SECS) {
            return Ok(cached.last_login_by_email.clone());
        }
    }

    let now = Utc::now();
    let start = now - Duration::days(AUDIT_WINDOW_DAYS);
    let csv = client
        .fetch_login_audit_csv(config_id, config, start, now)
        .await
        .map_err(|e| scope_error(e, "audit.read"))?;

    let last_login_by_email = parse_login_audit_csv(&csv).map_err(|e| {
        tracing::error!("parse login audit csv: {e}");
        (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "error": "csv_columns_not_recognized",
                "message": e,
            })),
        )
    })?;

    guard.insert(
        tenant_id,
        CachedActivity {
            fetched_at: now,
            last_login_by_email: last_login_by_email.clone(),
        },
    );

    Ok(last_login_by_email)
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
                    "LINE WORKS Developer Console で {scope} scope を追加してください (audit.read は監査の管理者権限も必要)"
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
}

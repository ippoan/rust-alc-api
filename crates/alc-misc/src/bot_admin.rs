/// Bot Config の開発者向け export (`GET /admin/bot/configs/export`)。
///
/// 一覧・保存・削除の口は ippoan/alc-lineworks-worker へ移した (W2c、Refs #680)。export だけは
/// auth-worker の `handleBotConfigExport` がここを呼ぶので残す (開発者だけ。`DEVELOPER_EMAILS`)。
use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Extension, Json, Router,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use alc_core::auth_middleware::AuthUser;
use alc_core::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/admin/bot/configs/export", get(export_configs))
}

// ---------------------------------------------------------------------------
// Developer-only export (Bot Config dump)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ExportQuery {
    tenant_id: Uuid,
}

/// `DEVELOPER_EMAILS` env var (comma-separated) のいずれかと一致するか判定。
/// 大文字小文字無視 / 空エントリ無視。env が未設定なら常に false (export 不可)。
pub(crate) fn is_developer_email(email: &str) -> bool {
    let allowlist = std::env::var("DEVELOPER_EMAILS").unwrap_or_default();
    allowlist
        .split(',')
        .map(str::trim)
        .any(|entry| !entry.is_empty() && entry.eq_ignore_ascii_case(email))
}

async fn export_configs(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Query(params): Query<ExportQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !is_developer_email(&auth_user.email) {
        tracing::warn!("export refused: {}", auth_user.email);
        return Err(StatusCode::FORBIDDEN);
    }

    let tid = params.tenant_id;

    let tenant = state
        .bot_admin
        .get_tenant_for_export(tid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch tenant for export: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let bot_configs = state
        .bot_admin
        .list_configs_for_export(tid)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch bot_configs for export: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(json!({
        "version": 1,
        "exported_at": Utc::now().to_rfc3339(),
        "tenant_id": tid.to_string(),
        "data": {
            "tenant": tenant,
            "users": [],
            "employees": [],
            "devices": [],
            "tenko_schedules": [],
            "webhook_configs": [],
            "tenant_allowed_emails": [],
            "sso_provider_configs": [],
            "tenko_call_numbers": [],
            "tenko_call_drivers": [],
            "bot_configs": bot_configs,
            "notify_line_configs": [],
            "notify_recipients": []
        }
    })))
}

#[cfg(test)]
mod tests {
    use super::is_developer_email;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_env<F: FnOnce()>(value: Option<&str>, body: F) {
        let _g = ENV_LOCK.lock().unwrap();
        let prev = std::env::var("DEVELOPER_EMAILS").ok();
        match value {
            Some(v) => std::env::set_var("DEVELOPER_EMAILS", v),
            None => std::env::remove_var("DEVELOPER_EMAILS"),
        }
        body();
        match prev {
            Some(v) => std::env::set_var("DEVELOPER_EMAILS", v),
            None => std::env::remove_var("DEVELOPER_EMAILS"),
        }
    }

    #[test]
    fn rejects_when_env_unset() {
        with_env(None, || {
            assert!(!is_developer_email("m.tama.ramu@gmail.com"));
        });
    }

    #[test]
    fn rejects_when_env_empty() {
        with_env(Some(""), || {
            assert!(!is_developer_email("m.tama.ramu@gmail.com"));
        });
    }

    #[test]
    fn accepts_single_match_case_insensitive() {
        with_env(Some("m.tama.ramu@gmail.com"), || {
            assert!(is_developer_email("m.tama.ramu@gmail.com"));
            assert!(is_developer_email("M.Tama.Ramu@Gmail.com"));
            assert!(!is_developer_email("attacker@example.com"));
        });
    }

    #[test]
    fn accepts_csv_with_whitespace() {
        with_env(
            Some(" foo@example.com , m.tama.ramu@gmail.com ,bar@x"),
            || {
                assert!(is_developer_email("foo@example.com"));
                assert!(is_developer_email("m.tama.ramu@gmail.com"));
                assert!(is_developer_email("bar@x"));
                assert!(!is_developer_email("baz@x"));
            },
        );
    }

    #[test]
    fn ignores_empty_csv_entries() {
        with_env(Some(",,,m.tama.ramu@gmail.com,,"), || {
            assert!(is_developer_email("m.tama.ramu@gmail.com"));
            assert!(!is_developer_email(""));
        });
    }

    #[test]
    fn restores_previous_value_from_some() {
        // Pre-set DEVELOPER_EMAILS so with_env's `prev` is Some(_) and the
        // restore path takes the `Some(v) => set_var(...)` branch.
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("DEVELOPER_EMAILS", "preset@example.com");
        drop(_g);

        with_env(Some("temp@example.com"), || {
            assert!(is_developer_email("temp@example.com"));
        });

        let _g = ENV_LOCK.lock().unwrap();
        assert_eq!(
            std::env::var("DEVELOPER_EMAILS").ok().as_deref(),
            Some("preset@example.com")
        );
        std::env::remove_var("DEVELOPER_EMAILS");
    }
}

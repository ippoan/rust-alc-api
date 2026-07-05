// Re-export route modules from domain crates
pub use alc_auth as auth;
pub use alc_carins::car_inspection_files;
pub use alc_carins::car_inspections;
pub use alc_carins::carins_files;
pub use alc_carins::nfc_tags;
pub use alc_devices::devices;
pub use alc_dtako::dtako_csv_proxy;
pub use alc_dtako::dtako_daily_hours;
pub use alc_dtako::dtako_drivers;
pub use alc_dtako::dtako_event_classifications;
pub use alc_dtako::dtako_logs;
pub use alc_dtako::dtako_operations;
pub use alc_dtako::dtako_restraint_report;
pub use alc_dtako::dtako_restraint_report_pdf;
pub use alc_dtako::dtako_scraper;
pub use alc_dtako::dtako_tickets;
pub use alc_dtako::dtako_upload;
pub use alc_dtako::dtako_vehicles;
pub use alc_dtako::dtako_work_times;
pub use alc_dtako::dtako_y_time_export;
pub use alc_dtako::vehicle_settings_dumps;
pub use alc_misc::access_requests;
pub use alc_misc::api_tokens;
pub use alc_misc::bot_admin;
pub use alc_misc::carrying_items;
pub use alc_misc::communication_items;
pub use alc_misc::employees;
pub use alc_misc::guidance_records;
pub use alc_misc::health;
pub use alc_misc::items;
pub use alc_misc::measurements;
pub use alc_misc::members;
pub use alc_misc::sso_admin;
pub use alc_misc::staging;
pub use alc_misc::tenant_users;
pub use alc_misc::timecard;
pub use alc_misc::upload;
pub use alc_notify::distribute as notify_distribute;
pub use alc_notify::documents as notify_documents;
pub use alc_notify::email_documents as notify_email_documents;
pub use alc_notify::groups as notify_groups;
pub use alc_notify::ingest as notify_ingest;
pub use alc_notify::line_config as notify_line_config;
pub use alc_notify::line_webhook as notify_line_webhook;
pub use alc_notify::lineworks_channels as notify_lineworks_channels;
pub use alc_notify::lineworks_directory as notify_lineworks_directory;
pub use alc_notify::read_tracker as notify_read_tracker;
pub use alc_notify::recipients as notify_recipients;
pub use alc_notify::test_endpoints as notify_test_endpoints;
pub use alc_notify::viewer as notify_viewer;
pub use alc_tenko::daily_health;
pub use alc_tenko::driver_info;
pub use alc_tenko::equipment_failures;
pub use alc_tenko::health_baselines;
pub use alc_tenko::tenko_call;
pub use alc_tenko::tenko_records;
pub use alc_tenko::tenko_schedules;
pub use alc_tenko::tenko_sessions;
pub use alc_tenko::tenko_webhooks;
pub use alc_trouble::categories as trouble_categories;
pub use alc_trouble::field_layouts as trouble_field_layouts;
pub use alc_trouble::files as trouble_files;
pub use alc_trouble::lineworks_members as trouble_lineworks_members;
pub use alc_trouble::notifications as trouble_notifications;
pub use alc_trouble::offices as trouble_offices;
pub use alc_trouble::progress_statuses as trouble_progress_statuses;
pub use alc_trouble::schedules as trouble_schedules;
pub use alc_trouble::task_statuses as trouble_task_statuses;
pub use alc_trouble::task_types as trouble_task_types;
pub use alc_trouble::tasks as trouble_tasks;
pub use alc_trouble::tickets as trouble_tickets;
pub use alc_trouble::workflow as trouble_workflow;

use axum::{middleware as axum_middleware, Extension, Router};

use crate::auth::google::GoogleTokenVerifier;
use crate::auth::jwt::INTERNAL_AUD;
use crate::middleware::auth::{require_internal_jwt, require_tenant_header, InternalOidcTrust};
use crate::AppState;

/// prod 用の `InternalOidcTrust` を構築する (aud=alc-api-internal、JWKS RS256 検証)。
/// main.rs から `router()` に渡す。テストは `GoogleTokenVerifier::with_test_claims`
/// で構築した trust を渡す (Refs #479 — HS256 dual-accept 撤去で OIDC 一本化。
/// DI 引数化したのは、Extension を router 内部で固定すると外側 layer で上書き
/// できず、テストが実 JWKS 検証を通せないため)。
pub fn internal_oidc_trust() -> InternalOidcTrust {
    InternalOidcTrust {
        verifier: GoogleTokenVerifier::new(INTERNAL_AUD.to_string()),
    }
}

pub fn router(
    internal_oidc: InternalOidcTrust,
    tenko_state: alc_tenko::TenkoState,
) -> Router<AppState> {
    // 管理者ルート — 注入 identity (X-User-*) を信頼 (Refs #434)。
    // 前段 proxy / gateway が introspect 検証済みの identity を注入する前提。
    // role 判定は各ハンドラが AuthUser から行う。
    let jwt_protected = Router::new()
        .merge(auth::protected_router())
        .merge(access_requests::protected_router())
        .merge(sso_admin::router())
        .merge(bot_admin::router())
        .merge(tenant_users::router())
        .merge(members::router())
        .merge(api_tokens::router())
        .layer(axum_middleware::from_fn(require_tenant_header));

    // 内部 API ルート (auth-worker 等の信頼できる呼び出し元のみ、aud=alc-api-internal)
    let internal_protected = Router::new()
        .merge(notify_lineworks_channels::internal_router())
        .merge(notify_viewer::internal_router())
        .merge(notify_line_webhook::internal_router())
        .merge(trouble_schedules::internal_fire_router())
        .merge(auth::internal_router())
        .layer(axum_middleware::from_fn(require_internal_jwt))
        // OIDC 検証設定 (Refs #479 — HS256 dual-accept 撤去で OIDC 一本化)。
        // require_internal_jwt の外側に置き、ハンドラ実行時に Extension を解決
        // できるようにする。verifier は呼び出し元 (main.rs = 実 JWKS / テスト =
        // with_test_claims) が注入する。
        .layer(Extension(internal_oidc));

    // テナント対応ルート — 注入 identity (X-Tenant-ID + 任意 X-User-*) を信頼 (Refs #434)。
    // 旧 require_tenant の bare X-Tenant-ID フォールバック / ローカル JWT 検証は撤去。
    // キオスク経路も含め前段 proxy が introspect 検証 → header 注入する。
    let tenant_protected = Router::new()
        .merge(employees::tenant_router())
        .merge(measurements::router())
        .merge(measurements::tenant_router())
        .merge(upload::tenant_router())
        .merge(timecard::tenant_router())
        .merge(devices::tenant_router())
        .merge(car_inspections::tenant_router())
        .merge(car_inspection_files::tenant_router())
        .merge(carins_files::tenant_router())
        .merge(nfc_tags::tenant_router())
        .merge(carrying_items::tenant_router())
        .merge(communication_items::tenant_router())
        .merge(guidance_records::tenant_router())
        .merge(items::tenant_router())
        .merge(dtako_csv_proxy::tenant_router())
        .merge(dtako_drivers::tenant_router())
        .merge(dtako_operations::tenant_router())
        .merge(dtako_restraint_report::tenant_router())
        .merge(dtako_restraint_report_pdf::tenant_router())
        .merge(dtako_scraper::tenant_router())
        .merge(dtako_tickets::tenant_router())
        .merge(dtako_work_times::tenant_router())
        .merge(dtako_daily_hours::tenant_router())
        .merge(dtako_upload::tenant_router())
        .merge(dtako_vehicles::tenant_router())
        .merge(vehicle_settings_dumps::tenant_router())
        .merge(dtako_event_classifications::tenant_router())
        .merge(dtako_y_time_export::tenant_router())
        .nest("/dtako-logs", dtako_logs::tenant_router())
        .merge(notify_recipients::tenant_router())
        .merge(notify_groups::tenant_router())
        .merge(notify_lineworks_directory::tenant_router())
        .merge(notify_lineworks_channels::tenant_router())
        .merge(notify_documents::tenant_router())
        .merge(notify_distribute::tenant_router())
        .merge(notify_email_documents::tenant_router())
        .merge(notify_test_endpoints::tenant_router())
        .merge(notify_line_config::tenant_router())
        .merge(trouble_tickets::tenant_router())
        .merge(trouble_files::tenant_router())
        .merge(trouble_workflow::tenant_router())
        .merge(trouble_categories::tenant_router())
        .merge(trouble_offices::tenant_router())
        .merge(trouble_progress_statuses::tenant_router())
        .merge(trouble_notifications::tenant_router())
        .merge(trouble_schedules::tenant_router())
        .merge(trouble_tasks::tenant_router())
        .merge(trouble_task_types::tenant_router())
        .merge(trouble_task_statuses::tenant_router())
        .merge(trouble_field_layouts::tenant_router())
        .merge(trouble_lineworks_members::tenant_router())
        .layer(axum_middleware::from_fn(require_tenant_header));

    // 公開ルート (認証不要)。旧ログイン経路 (auth::public_router = Google /
    // LINE / LINE WORKS OAuth / refresh / password login) は auth-worker へ
    // 完全移管したため撤去 (Refs #479 PR-3)。
    let public_routes = Router::new()
        .merge(health::router())
        .merge(devices::public_router())
        .merge(staging::router())
        .merge(notify_ingest::public_router())
        .merge(notify_line_webhook::public_router())
        .merge(notify_read_tracker::public_router())
        .merge(notify_viewer::public_router())
        .merge(access_requests::public_router())
        .merge(dtako_tickets::public_close_router());
    // #434 lockdown: trouble schedule fire は internal_protected へ移動
    // (`/api/internal/trouble/schedules/{id}/fire`)。bare public 経路は廃止。

    // tenko ドメイン (Refs #513) — AppState から分離した TenkoState でマウントする。
    // tenant 系ルートには monolith 本体と同じ require_tenant_header を張る。
    let tenko_tenant: Router<AppState> = Router::new()
        .merge(tenko_schedules::tenant_router())
        .merge(tenko_sessions::tenant_router())
        .merge(tenko_records::tenant_router())
        .merge(health_baselines::tenant_router())
        .merge(equipment_failures::tenant_router())
        .merge(tenko_webhooks::tenant_router())
        .merge(tenko_call::tenant_router())
        .merge(daily_health::tenant_router())
        .merge(driver_info::tenant_router())
        .layer(axum_middleware::from_fn(require_tenant_header))
        .with_state(tenko_state.clone());
    let tenko_public: Router<AppState> = tenko_call::public_router().with_state(tenko_state);

    Router::new()
        .merge(public_routes)
        .merge(tenko_public)
        .merge(jwt_protected)
        .merge(internal_protected)
        .merge(tenant_protected)
        .merge(tenko_tenant)
}

/// email-receiver Worker から `POST /api/dtako/tickets` を受ける internal ingest
/// router。`INTERNAL_SHARED_SECRET` env が空 (= 未配布) なら呼出し側で disable
/// する (= caller が `internal_shared_secret_router(None)` を渡せば empty router、
/// `Some(secret)` を渡せば middleware + extension 付きで実装する safe fallback、
/// Refs ippoan/email-receiver#1)。
///
/// 本関数自体は env を読まない (= テスト容易性 + main.rs 側で env を読み caller が
/// branch する設計、`src/main.rs` の deploy/staging 区分けと同様)。
pub fn internal_shared_secret_router(internal_secret: Option<String>) -> Router<AppState> {
    match internal_secret {
        Some(secret) if !secret.is_empty() => Router::new()
            .merge(dtako_tickets::internal_router())
            .layer(axum_middleware::from_fn(
                alc_core::auth_middleware::require_internal_shared_secret,
            ))
            .layer(axum::Extension(
                alc_core::auth_middleware::InternalSharedSecret(secret),
            )),
        _ => Router::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `internal_shared_secret_router` の両分岐 (`None` / 空 / Some 非空) を
    /// 呼ぶだけのカバレッジテスト。Router 自体の挙動は dtako-api の integration
    /// test がカバーする。ここでは Router build path が panic なく通る + 両分岐
    /// 行が llvm-cov に乗ることだけを保証する。
    #[test]
    fn internal_shared_secret_router_branches() {
        // None → empty Router (`_ =>` ブランチ)
        let _ = internal_shared_secret_router(None);
        // Some("") → empty Router (`_ =>` ブランチ、guard 不一致)
        let _ = internal_shared_secret_router(Some(String::new()));
        // Some(non-empty) → middleware + extension 付き Router (`Some(secret)` ブランチ)
        let _ = internal_shared_secret_router(Some("test-secret".to_string()));
    }

    /// `internal_oidc_trust()` (prod 用 OIDC trust 構築) のカバレッジテスト
    /// (Refs #479)。verifier の検証挙動自体は alc-core の auth_middleware /
    /// auth_google テストがカバーする。ここでは prod 構築 path が panic なく
    /// 通ることだけを保証する (JWKS fetch は verify 時まで遅延されるため
    /// ネットワーク非依存)。
    #[test]
    fn internal_oidc_trust_builds() {
        let _ = internal_oidc_trust();
    }
}

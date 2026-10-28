// Re-export route modules from domain crates
pub use alc_misc::bot_admin;
pub use alc_misc::health;
pub use alc_notify::documents as notify_documents;
pub use alc_notify::email_documents as notify_email_documents;
pub use alc_notify::ingest as notify_ingest;
pub use alc_notify::line_webhook as notify_line_webhook;
pub use alc_notify::lineworks_channels as notify_lineworks_channels;
pub use alc_notify::test_endpoints as notify_test_endpoints;

pub mod rls_check;

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

pub fn router(internal_oidc: InternalOidcTrust) -> Router<AppState> {
    // 管理者ルート — 注入 identity (X-User-*) を信頼 (Refs #434)。
    // 前段 proxy / gateway が introspect 検証済みの identity を注入する前提。
    // role 判定は各ハンドラが AuthUser から行う。
    let jwt_protected = Router::new()
        .merge(bot_admin::router())
        .layer(axum_middleware::from_fn(require_tenant_header));

    // 内部 API ルート (auth-worker 等の信頼できる呼び出し元のみ、aud=alc-api-internal)
    let internal_protected = Router::new()
        .merge(notify_lineworks_channels::internal_router())
        .merge(notify_line_webhook::internal_router())
        // device_id から有効な端末の tenant を返す内部口。auth-worker の
        // pair-internal が tenant 決定に使う (Refs ippoan/auth-worker#544)。
        // RLS が効いているかの固定の検査 (引数なし・読み取りだけ、Refs ippoan/auth-worker#605)。
        .merge(rls_check::internal_router())
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
        .merge(notify_documents::tenant_router())
        .merge(notify_email_documents::tenant_router())
        .merge(notify_test_endpoints::tenant_router())
        .layer(axum_middleware::from_fn(require_tenant_header));

    // 公開ルート (認証不要)。旧ログイン経路 (auth::public_router = Google /
    // LINE / LINE WORKS OAuth / refresh / password login) は auth-worker へ
    // 完全移管したため撤去 (Refs #479 PR-3)。
    let public_routes = Router::new()
        .merge(health::router())
        .merge(notify_ingest::public_router())
        .merge(notify_line_webhook::public_router());

    Router::new()
        .merge(public_routes)
        .merge(jwt_protected)
        .merge(internal_protected)
        .merge(tenant_protected)
}

#[cfg(test)]
mod tests {
    use super::*;

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

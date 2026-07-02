//! auth_jwt のロジックは leaf crate `alc-auth-jwt` に切り出した (Refs #410)。
//!
//! JWT の発行・検証 (HS256、共有 `JWT_SECRET`) は auth-worker に完全移管され
//! (Refs #479 PR-3)、`JwtSecret` / `AppClaims` / `create_access_token` /
//! `verify_access_token` / refresh token hash 関数群は撤去済み。
//! 本 module は後方互換のための薄い re-export shim で、残るのは internal OIDC
//! 検証 (`auth_middleware::require_internal_jwt`) が使う `INTERNAL_AUD` のみ
//! (= 既存 `alc_core::auth_jwt::INTERNAL_AUD` import が無変更で通る)。

pub use alc_auth_jwt::INTERNAL_AUD;

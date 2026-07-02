//! 薄い leaf crate: 認証 JWT 関連で rust 側に残る共有定数のみを持つ。
//!
//! かつては app / internal JWT の発行・検証 (HS256、共有 `JWT_SECRET`) と
//! refresh token hash をここに置いていたが (Refs #410)、JWT の発行・検証は
//! auth-worker に完全移管された (Refs #479 PR-3)。rust 側で JWT を組み立てる /
//! 剥がすコードは存在せず、本 crate に残るのは internal OIDC 検証
//! (`alc-core::auth_middleware::require_internal_jwt`) が audience として使う
//! `INTERNAL_AUD` だけになった。
//!
//! `alc-core::auth_jwt` がこの crate を re-export しているため、
//! repo 内の既存 import (`alc_core::auth_jwt::INTERNAL_AUD`) は無変更で通る。

/// 内部 API 用 OIDC token の audience (auth-worker → rust-alc-api 間の internal call)。
///
/// 通常のデータ API 用 OIDC (aud=service URL) と区別するため、`/api/internal/*`
/// は `aud = "alc-api-internal"` を強制する。auth-worker / Cloud Scheduler が
/// この audience で mint した Google OIDC token だけが `require_internal_jwt`
/// を通過できる (confused-deputy 防止)。
pub const INTERNAL_AUD: &str = "alc-api-internal";

#[cfg(test)]
mod tests {
    use super::*;

    /// audience 文字列は auth-worker 側の mint 設定 (Cloud Run custom audiences)
    /// と一致している必要がある wire contract なので、値を test で固定する。
    #[test]
    fn internal_aud_is_pinned() {
        assert_eq!(INTERNAL_AUD, "alc-api-internal");
    }
}

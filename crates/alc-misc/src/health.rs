use axum::{routing::get, Json, Router};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use alc_core::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(health_check)).route(
        "/health/internal-secret-fingerprints",
        get(internal_secret_fingerprints),
    )
}

async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "service": "alc-api",
        "git_sha": option_env!("GIT_SHA").unwrap_or("dev"),
        "git_ref": option_env!("GIT_REF").unwrap_or(""),
    }))
}

/// `GET /health/internal-secret-fingerprints` — backend が Cloud Run env から
/// 解決した全 `INTERNAL_SHARED_SECRET*` の非可逆 fingerprint を返す。
///
/// 用途: cross-store drift (CF Secrets Store ↔ GCP Secret Manager で同名 secret の
/// 値が乖離) の切り分け。auth-worker の `/health/internal-secret-fingerprints`
/// (= CF Secrets Store binding の hash) や email-receiver Worker log の fingerprint
/// と直接突合できる。
///
/// 値そのものは context にも response にも一切載せない:
///   - hex 8 文字 = 32 bit、SHA-256 の prefix なので不可逆 (preimage 不可能)
///   - length / head / tail は出さない (partial leak 防止)
///
/// 認証なし (公開) で問題ないか:
///   - prefix 単独では値復元不可
///   - env 名は CLAUDE.md / cloudrun/render.sh で既に公開
///   - 攻撃者にとっての追加情報は実質ゼロ
///
/// Refs ippoan/email-receiver#1
async fn internal_secret_fingerprints() -> Json<Value> {
    let mut bindings: Map<String, Value> = Map::new();
    for (key, value) in std::env::vars() {
        if !key.starts_with("INTERNAL_SHARED_SECRET") {
            continue;
        }
        if value.is_empty() {
            continue;
        }
        bindings.insert(key, Value::String(sha256_prefix(&value)));
    }
    Json(json!({
        "service": "alc-api",
        "version": env!("CARGO_PKG_VERSION"),
        "bindings": bindings,
    }))
}

fn sha256_prefix(input: &str) -> String {
    let mut h = Sha256::new();
    h.update(input.as_bytes());
    let digest = h.finalize();
    let hex = digest
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    hex[..8].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // env vars はプロセス共有なので、本ファイルの env 操作は逐次化する。
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn sha256_prefix_returns_hex_8_chars_and_is_deterministic() {
        let a = sha256_prefix("hello");
        let b = sha256_prefix("hello");
        assert_eq!(a, b);
        assert_eq!(a.len(), 8);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, sha256_prefix("hellp"));
        // 既知ベクター (SHA-256("hello") = 2cf24dba5fb0a30e...) の prefix
        assert_eq!(sha256_prefix("hello"), "2cf24dba");
    }

    #[tokio::test]
    async fn health_endpoint_returns_ok() {
        let Json(body) = health_check().await;
        assert_eq!(body["status"], "ok");
        assert_eq!(body["service"], "alc-api");
        assert!(body["version"].is_string());
    }

    #[tokio::test]
    async fn fingerprints_endpoint_lists_internal_shared_secret_bindings_and_skips_empty_and_unrelated(
    ) {
        let _g = ENV_LOCK.lock().unwrap();
        std::env::set_var("INTERNAL_SHARED_SECRET_TEST_FP_A", "abc");
        std::env::set_var("INTERNAL_SHARED_SECRET_TEST_FP_B", "xyz");
        std::env::set_var("INTERNAL_SHARED_SECRET_TEST_FP_EMPTY", "");
        std::env::set_var("UNRELATED_FP_TEST_VAR", "should-not-leak");

        let Json(body) = internal_secret_fingerprints().await;
        assert_eq!(body["service"], "alc-api");
        let bindings = body["bindings"].as_object().unwrap();
        assert_eq!(
            bindings.get("INTERNAL_SHARED_SECRET_TEST_FP_A").unwrap(),
            &Value::String(sha256_prefix("abc")),
        );
        assert_eq!(
            bindings.get("INTERNAL_SHARED_SECRET_TEST_FP_B").unwrap(),
            &Value::String(sha256_prefix("xyz")),
        );
        // 空値の binding は出さない (運用上 noise になるため)
        assert!(bindings
            .get("INTERNAL_SHARED_SECRET_TEST_FP_EMPTY")
            .is_none());
        // prefix 違いの env は混ぜない
        assert!(!bindings.contains_key("UNRELATED_FP_TEST_VAR"));

        std::env::remove_var("INTERNAL_SHARED_SECRET_TEST_FP_A");
        std::env::remove_var("INTERNAL_SHARED_SECRET_TEST_FP_B");
        std::env::remove_var("INTERNAL_SHARED_SECRET_TEST_FP_EMPTY");
        std::env::remove_var("UNRELATED_FP_TEST_VAR");
    }

    #[test]
    fn router_mounts_both_routes() {
        // router() の構築自体が panic しないこと + 両 route が存在することを最低限担保
        let _r: Router<alc_core::AppState> = router();
    }
}

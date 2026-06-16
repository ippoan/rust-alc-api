use axum::{extract::Query, routing::get, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use alc_core::constant_time::constant_time_eq;
use alc_core::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health_check))
        .route("/health/secret-fingerprint", get(secret_fingerprint))
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

#[derive(Debug, Deserialize)]
pub(crate) struct SecretFingerprintQuery {
    name: String,
    expected: String,
}

/// `GET /health/secret-fingerprint?name=<env>&expected=<8hex>` —
/// backend が Cloud Run env から解決した任意 env の sha256[0..8] が
/// `expected` と一致するかを `{"match": bool}` で返す。
///
/// 用途: cross-store drift (= GCP Secret Manager と Cloud Run env (= secretKeyRef
/// 解決済み runtime 値) で同名 secret の値が乖離) の CI 自動検出。caller 側
/// (ippoan/ci-workflows `drift-check.yml`) が GCP SM から値を読んで sha256[0..8]
/// を計算し、本 endpoint を叩いて `match: true` を assert する。
///
/// 値の hex を返さない (oracle 防止):
///   - env 不在 / 値違い / typo を全て `match: false` に集約 → name 列挙不可
///   - constant-time 比較 (`alc_core::constant_time::constant_time_eq`)
///
/// query 形式違反は 400 で reject (= 200/match:false に丸めない)。不正 query は
/// drift とは別 class の bug なので CI に切り分けさせたい。
///
/// 認証不要 (= CCoW / CI runner から curl 一発):
///   - `expected` は 32 bit の sha256 prefix なので preimage 不可
///   - env 名は CLAUDE.md / cloudrun/render.sh で既に公開
///   - 攻撃者にとっての追加情報は実質ゼロ
///
/// Refs ippoan/rust-alc-api#424 / ippoan/email-receiver#1
async fn secret_fingerprint(
    Query(q): Query<SecretFingerprintQuery>,
) -> (axum::http::StatusCode, Json<Value>) {
    // GCP Secret Manager の secret 名規約 + 一般的な env 名にも合致する制限。
    // shell 経由で渡される実行可能 char もこの validation で reject される。
    if q.name.is_empty()
        || q.name.len() > 255
        || !q
            .name
            .chars()
            .next()
            .map_or(false, |c| c.is_ascii_alphabetic())
        || !q
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid name"})),
        );
    }
    if q.expected.len() != 8
        || !q
            .expected
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid expected"})),
        );
    }
    let actual = match std::env::var(&q.name) {
        Ok(v) if !v.is_empty() => sha256_prefix(&v),
        // env 不在 / 空値は match:false に集約 (oracle 不可)。
        _ => String::new(),
    };
    let match_ok = !actual.is_empty() && constant_time_eq(actual.as_bytes(), q.expected.as_bytes());
    (
        axum::http::StatusCode::OK,
        Json(json!({ "match": match_ok })),
    )
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
    use tokio::sync::Mutex;

    // env vars はプロセス共有なので、本ファイルの env 操作は逐次化する。
    // tokio::sync::Mutex を使うのは、std::sync::Mutex の guard を `.await` 越しに
    // 保持すると clippy::await_holding_lock (= CI で `-D warnings` により error)
    // を踏むため。
    static ENV_LOCK: Mutex<()> = Mutex::const_new(());

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
    async fn secret_fingerprint_returns_match_true_when_env_present_and_expected_matches() {
        let _g = ENV_LOCK.lock().await;
        std::env::set_var("INTERNAL_SHARED_SECRET_TEST_FP_OK", "hello");
        let expected = sha256_prefix("hello");
        let (status, Json(body)) = secret_fingerprint(Query(SecretFingerprintQuery {
            name: "INTERNAL_SHARED_SECRET_TEST_FP_OK".to_string(),
            expected: expected.clone(),
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(body, json!({"match": true}));
        // hex の値そのものは response に echo しない
        assert!(!body.to_string().contains(&expected));
        std::env::remove_var("INTERNAL_SHARED_SECRET_TEST_FP_OK");
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_match_false_when_expected_differs() {
        let _g = ENV_LOCK.lock().await;
        std::env::set_var("INTERNAL_SHARED_SECRET_TEST_FP_DIFF", "hello");
        let (status, Json(body)) = secret_fingerprint(Query(SecretFingerprintQuery {
            name: "INTERNAL_SHARED_SECRET_TEST_FP_DIFF".to_string(),
            expected: "deadbeef".to_string(),
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(body, json!({"match": false}));
        std::env::remove_var("INTERNAL_SHARED_SECRET_TEST_FP_DIFF");
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_match_false_when_env_missing_no_oracle_on_typo() {
        let _g = ENV_LOCK.lock().await;
        // 値の sha と一致しても、name typo なら必ず false にする
        let (status, Json(body)) = secret_fingerprint(Query(SecretFingerprintQuery {
            name: "INTERNAL_SHARED_SECRET_TYPO_NOT_SET".to_string(),
            expected: "2cf24dba".to_string(),
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(body, json!({"match": false}));
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_match_false_when_env_value_is_empty() {
        let _g = ENV_LOCK.lock().await;
        std::env::set_var("INTERNAL_SHARED_SECRET_TEST_FP_EMPTY", "");
        let (status, Json(body)) = secret_fingerprint(Query(SecretFingerprintQuery {
            name: "INTERNAL_SHARED_SECRET_TEST_FP_EMPTY".to_string(),
            // SHA-256("") = e3b0c44298fc1c14... の prefix
            expected: "e3b0c442".to_string(),
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(body, json!({"match": false}));
        std::env::remove_var("INTERNAL_SHARED_SECRET_TEST_FP_EMPTY");
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_400_on_invalid_name_empty() {
        let (status, Json(_body)) = secret_fingerprint(Query(SecretFingerprintQuery {
            name: "".to_string(),
            expected: "2cf24dba".to_string(),
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_400_on_invalid_name_leading_digit() {
        let (status, Json(_body)) = secret_fingerprint(Query(SecretFingerprintQuery {
            name: "1BAD".to_string(),
            expected: "2cf24dba".to_string(),
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_400_on_invalid_name_special_char() {
        let (status, Json(_body)) = secret_fingerprint(Query(SecretFingerprintQuery {
            name: "BAD;NAME".to_string(),
            expected: "2cf24dba".to_string(),
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_400_on_invalid_expected_wrong_length() {
        let (status, Json(_body)) = secret_fingerprint(Query(SecretFingerprintQuery {
            name: "INTERNAL_SHARED_SECRET".to_string(),
            expected: "deadbe".to_string(),
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_400_on_invalid_expected_non_hex() {
        let (status, Json(_body)) = secret_fingerprint(Query(SecretFingerprintQuery {
            name: "INTERNAL_SHARED_SECRET".to_string(),
            expected: "ZZZZZZZZ".to_string(),
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_400_on_uppercase_hex_expected() {
        // expected は lowercase 限定 (caller の sha256sum 出力に合わせる)
        let (status, Json(_body)) = secret_fingerprint(Query(SecretFingerprintQuery {
            name: "INTERNAL_SHARED_SECRET".to_string(),
            expected: "2CF24DBA".to_string(),
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_400_on_name_too_long() {
        let long_name = format!("A{}", "B".repeat(255));
        let (status, Json(_body)) = secret_fingerprint(Query(SecretFingerprintQuery {
            name: long_name,
            expected: "2cf24dba".to_string(),
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn secret_fingerprint_works_for_arbitrary_env_name_not_limited_to_internal_shared_secret()
    {
        let _g = ENV_LOCK.lock().await;
        std::env::set_var("UNRELATED_FP_TEST_ARBITRARY", "different-secret-here");
        let expected = sha256_prefix("different-secret-here");
        let (status, Json(body)) = secret_fingerprint(Query(SecretFingerprintQuery {
            name: "UNRELATED_FP_TEST_ARBITRARY".to_string(),
            expected,
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(body, json!({"match": true}));
        std::env::remove_var("UNRELATED_FP_TEST_ARBITRARY");
    }

    #[test]
    fn router_mounts_health_and_fingerprint_routes() {
        // router() の構築自体が panic しないこと
        let _r: Router<alc_core::AppState> = router();
    }
}

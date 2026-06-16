use axum::{extract::Query, routing::get, Json, Router};
use base64::prelude::*;
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
}

/// `GET /health/secret-fingerprint?name=<env>` —
/// Cloud Run runtime env (`std::env::var(name)`、= revision 作成時に secretKeyRef
/// で解決された値) と **GCP Secret Manager の現 latest version の値** を、
/// この service の runtime SA (= 既に当該 secret の `secretAccessor` を持っている)
/// で直接読み出して突合する。差異があれば `{"match": false}` を返す。
///
/// 用途: cross-store drift (= GCP SM が rotate されたのに Cloud Run の secretKeyRef
/// 解決値が古いまま、または Cloud Run env と GCP SM の値が乖離) の CI 自動検出。
/// CI runner は **WIF や gcloud を持たず** 単 curl で叩くだけで判定できる
/// (= staging service 自身が GCP に問い合わせる)。
///
/// 値の hex を返さない (oracle 防止):
///   - env 不在 / GCP unreachable / 値違い / typo は全て `match: false` に集約
///   - constant-time 比較 (`alc_core::constant_time::constant_time_eq`)
///
/// query 形式違反は 400 で reject (= 200/match:false に丸めない)。不正 query は
/// drift とは別 class の bug なので CI に切り分けさせたい。
///
/// 認証不要:
///   - GCP SM 値の hex を露出しない (`{match: bool}` のみ)
///   - env 名は CLAUDE.md / cloudrun/render.sh で既に公開
///   - 攻撃者にとっての追加情報は実質ゼロ
///
/// Refs ippoan/rust-alc-api#424 / ippoan/email-receiver#1
async fn secret_fingerprint(
    Query(q): Query<SecretFingerprintQuery>,
) -> (axum::http::StatusCode, Json<Value>) {
    if !valid_name(&q.name) {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid name"})),
        );
    }
    // env 解決失敗 / 空値 は match:false (oracle 不可)。
    let env_value = match std::env::var(&q.name) {
        Ok(v) if !v.is_empty() => v,
        _ => return (axum::http::StatusCode::OK, Json(json!({"match": false}))),
    };
    // GCP project は metadata server から取る (= asia-northeast1 Cloud Run でも
    // hardcode せず、staging/prod 両方で動く)。
    let project = match fetch_project_id().await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "secret-fingerprint: failed to resolve GCP project");
            return (axum::http::StatusCode::OK, Json(json!({"match": false})));
        }
    };
    let gcp_value = match fetch_gcp_secret(&project, &q.name).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, name = %q.name, "secret-fingerprint: failed to fetch GCP SM value");
            return (axum::http::StatusCode::OK, Json(json!({"match": false})));
        }
    };
    let env_h = sha256_prefix(&env_value);
    let gcp_h = sha256_prefix(&gcp_value);
    let match_ok = constant_time_eq(env_h.as_bytes(), gcp_h.as_bytes());
    (
        axum::http::StatusCode::OK,
        Json(json!({ "match": match_ok })),
    )
}

fn valid_name(name: &str) -> bool {
    // GCP Secret Manager の名前規約 + 一般的な env 名にも合致する制限。
    // shell metachar を含む name を reject する sanity check も兼ねる。
    if name.is_empty() || name.len() > 255 {
        return false;
    }
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
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

/// metadata server の base URL を tests から差し替えるための hook。
/// 通常は GCE/Cloud Run の固定 host を返す。
static METADATA_BASE_URL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
/// Secret Manager API の base URL を tests から差し替えるための hook。
static SECRETMANAGER_BASE_URL: std::sync::OnceLock<String> = std::sync::OnceLock::new();

fn metadata_base() -> &'static str {
    METADATA_BASE_URL
        .get()
        .map(|s| s.as_str())
        .unwrap_or("http://metadata.google.internal")
}

fn secretmanager_base() -> &'static str {
    SECRETMANAGER_BASE_URL
        .get()
        .map(|s| s.as_str())
        .unwrap_or("https://secretmanager.googleapis.com")
}

async fn fetch_project_id() -> Result<String, String> {
    let url = format!("{}/computeMetadata/v1/project/project-id", metadata_base());
    let resp = reqwest::Client::new()
        .get(&url)
        .header("Metadata-Flavor", "Google")
        .send()
        .await
        .map_err(|e| format!("metadata: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("metadata project-id status={}", resp.status()));
    }
    resp.text().await.map_err(|e| format!("metadata body: {e}"))
}

async fn fetch_access_token() -> Result<String, String> {
    let url = format!(
        "{}/computeMetadata/v1/instance/service-accounts/default/token",
        metadata_base()
    );
    let json: Value = reqwest::Client::new()
        .get(&url)
        .header("Metadata-Flavor", "Google")
        .send()
        .await
        .map_err(|e| format!("metadata: {e}"))?
        .json()
        .await
        .map_err(|e| format!("metadata json: {e}"))?;
    json["access_token"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "no access_token in metadata response".to_string())
}

async fn fetch_gcp_secret(project: &str, name: &str) -> Result<String, String> {
    let token = fetch_access_token().await?;
    let url = format!(
        "{}/v1/projects/{}/secrets/{}/versions/latest:access",
        secretmanager_base(),
        project,
        name
    );
    let resp = reqwest::Client::new()
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("sm: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("sm status={}", resp.status()));
    }
    let json: Value = resp.json().await.map_err(|e| format!("sm json: {e}"))?;
    let b64 = json["payload"]["data"]
        .as_str()
        .ok_or_else(|| "no payload.data in SM response".to_string())?;
    let bytes = BASE64_STANDARD
        .decode(b64)
        .map_err(|e| format!("sm base64: {e}"))?;
    String::from_utf8(bytes).map_err(|e| format!("sm utf8: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::Mutex;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // env vars と OnceLock の base URL はプロセス共有なのでテスト間で逐次化する。
    static ENV_LOCK: Mutex<()> = Mutex::const_new(());

    fn set_base_urls_once(metadata: &str, sm: &str) {
        // OnceLock は 1 度しか set できない。テスト全体で 1 つの mock server を
        // 共有する設計にして OK (各テストは独立 mock を mount する)。
        let _ = METADATA_BASE_URL.set(metadata.to_string());
        let _ = SECRETMANAGER_BASE_URL.set(sm.to_string());
    }

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

    #[test]
    fn valid_name_rejects_invalid_inputs() {
        assert!(valid_name("INTERNAL_SHARED_SECRET"));
        assert!(valid_name("JWT_SECRET"));
        assert!(valid_name("a"));
        assert!(valid_name("A-B_C"));
        assert!(!valid_name(""));
        assert!(!valid_name("1BAD"));
        assert!(!valid_name("BAD;NAME"));
        assert!(!valid_name("bad name"));
        assert!(!valid_name(&format!("A{}", "B".repeat(255))));
    }

    #[tokio::test]
    async fn health_endpoint_returns_ok() {
        let Json(body) = health_check().await;
        assert_eq!(body["status"], "ok");
        assert_eq!(body["service"], "alc-api");
        assert!(body["version"].is_string());
    }

    async fn setup_mock_server() -> MockServer {
        let server = MockServer::start().await;
        // metadata server: project-id + access token
        Mock::given(method("GET"))
            .and(path("/computeMetadata/v1/project/project-id"))
            .and(header("Metadata-Flavor", "Google"))
            .respond_with(ResponseTemplate::new(200).set_body_string("cloudsql-sv"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(
                "/computeMetadata/v1/instance/service-accounts/default/token",
            ))
            .and(header("Metadata-Flavor", "Google"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"access_token": "tok-xyz", "expires_in": 3600})),
            )
            .mount(&server)
            .await;
        server
    }

    fn sm_payload(value: &str) -> Value {
        // GCP SM REST returns base64-encoded `payload.data`.
        json!({"payload": {"data": BASE64_STANDARD.encode(value.as_bytes())}})
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_match_true_when_env_and_gcp_agree() {
        let _g = ENV_LOCK.lock().await;
        let server = setup_mock_server().await;
        set_base_urls_once(&server.uri(), &server.uri());

        Mock::given(method("GET"))
            .and(path(
                "/v1/projects/cloudsql-sv/secrets/INTERNAL_SHARED_SECRET_TEST_FP_OK/versions/latest:access",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(sm_payload("hello")))
            .mount(&server)
            .await;

        std::env::set_var("INTERNAL_SHARED_SECRET_TEST_FP_OK", "hello");
        let (status, Json(body)) = secret_fingerprint(Query(SecretFingerprintQuery {
            name: "INTERNAL_SHARED_SECRET_TEST_FP_OK".to_string(),
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(body, json!({"match": true}));
        std::env::remove_var("INTERNAL_SHARED_SECRET_TEST_FP_OK");
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_match_false_when_env_and_gcp_differ() {
        let _g = ENV_LOCK.lock().await;
        let server = setup_mock_server().await;
        set_base_urls_once(&server.uri(), &server.uri());

        Mock::given(method("GET"))
            .and(path(
                "/v1/projects/cloudsql-sv/secrets/INTERNAL_SHARED_SECRET_TEST_FP_DIFF/versions/latest:access",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(sm_payload("rotated-value")))
            .mount(&server)
            .await;

        std::env::set_var("INTERNAL_SHARED_SECRET_TEST_FP_DIFF", "old-value");
        let (status, Json(body)) = secret_fingerprint(Query(SecretFingerprintQuery {
            name: "INTERNAL_SHARED_SECRET_TEST_FP_DIFF".to_string(),
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(body, json!({"match": false}));
        std::env::remove_var("INTERNAL_SHARED_SECRET_TEST_FP_DIFF");
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_match_false_when_env_missing() {
        let _g = ENV_LOCK.lock().await;
        let server = setup_mock_server().await;
        set_base_urls_once(&server.uri(), &server.uri());
        let (status, Json(body)) = secret_fingerprint(Query(SecretFingerprintQuery {
            name: "INTERNAL_SHARED_SECRET_NOT_SET_AT_ALL".to_string(),
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(body, json!({"match": false}));
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_match_false_when_gcp_returns_error() {
        let _g = ENV_LOCK.lock().await;
        let server = setup_mock_server().await;
        set_base_urls_once(&server.uri(), &server.uri());

        // SM API returns 403 (permission denied) — fallthrough to match:false.
        Mock::given(method("GET"))
            .and(path(
                "/v1/projects/cloudsql-sv/secrets/INTERNAL_SHARED_SECRET_TEST_FP_403/versions/latest:access",
            ))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        std::env::set_var("INTERNAL_SHARED_SECRET_TEST_FP_403", "x");
        let (status, Json(body)) = secret_fingerprint(Query(SecretFingerprintQuery {
            name: "INTERNAL_SHARED_SECRET_TEST_FP_403".to_string(),
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(body, json!({"match": false}));
        std::env::remove_var("INTERNAL_SHARED_SECRET_TEST_FP_403");
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_400_on_invalid_name() {
        let (status, Json(_body)) = secret_fingerprint(Query(SecretFingerprintQuery {
            name: "BAD;NAME".to_string(),
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_400_on_empty_name() {
        let (status, Json(_body)) = secret_fingerprint(Query(SecretFingerprintQuery {
            name: "".to_string(),
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn router_mounts_health_and_fingerprint_routes() {
        let _r: Router<alc_core::AppState> = router();
    }
}

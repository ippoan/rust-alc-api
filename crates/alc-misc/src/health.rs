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

/// GCP metadata / Secret Manager の REST API base URL を保持する小さなクライアント。
/// テストでは mock server の URL を渡し、本番では `Default` で固定 host を使う。
pub(crate) struct GcpSecretsClient {
    metadata_base: String,
    sm_base: String,
}

impl Default for GcpSecretsClient {
    fn default() -> Self {
        Self {
            metadata_base: "http://metadata.google.internal".to_string(),
            sm_base: "https://secretmanager.googleapis.com".to_string(),
        }
    }
}

impl GcpSecretsClient {
    async fn project_id(&self) -> Result<String, String> {
        let url = format!(
            "{}/computeMetadata/v1/project/project-id",
            self.metadata_base
        );
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

    async fn access_token(&self) -> Result<String, String> {
        let url = format!(
            "{}/computeMetadata/v1/instance/service-accounts/default/token",
            self.metadata_base
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

    async fn fetch_secret(&self, project: &str, name: &str) -> Result<String, String> {
        let token = self.access_token().await?;
        let url = format!(
            "{}/v1/projects/{}/secrets/{}/versions/latest:access",
            self.sm_base, project, name
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
/// Refs ippoan/rust-alc-api#424 / ippoan/email-receiver#1
async fn secret_fingerprint(
    Query(q): Query<SecretFingerprintQuery>,
) -> (axum::http::StatusCode, Json<Value>) {
    secret_fingerprint_impl(&q.name, &GcpSecretsClient::default()).await
}

async fn secret_fingerprint_impl(
    name: &str,
    gcp: &GcpSecretsClient,
) -> (axum::http::StatusCode, Json<Value>) {
    if !valid_name(name) {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid name"})),
        );
    }
    let env_value = match std::env::var(name) {
        Ok(v) if !v.is_empty() => v,
        _ => return ok_match(false),
    };
    let project = match gcp.project_id().await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "secret-fingerprint: failed to resolve GCP project");
            return ok_match(false);
        }
    };
    let gcp_value = match gcp.fetch_secret(&project, name).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, name = %name, "secret-fingerprint: failed to fetch GCP SM value");
            return ok_match(false);
        }
    };
    let env_h = sha256_prefix(&env_value);
    let gcp_h = sha256_prefix(&gcp_value);
    let match_ok = constant_time_eq(env_h.as_bytes(), gcp_h.as_bytes());
    ok_match(match_ok)
}

fn ok_match(m: bool) -> (axum::http::StatusCode, Json<Value>) {
    (axum::http::StatusCode::OK, Json(json!({ "match": m })))
}

fn valid_name(name: &str) -> bool {
    // GCP Secret Manager の名前規約 + 一般的な env 名にも合致する制限。
    // shell metachar を含む name を reject する sanity check も兼ねる。
    // 短絡評価で empty を先に弾けば、`as_bytes()[0]` は安全。
    !name.is_empty()
        && name.len() <= 255
        && name.as_bytes()[0].is_ascii_alphabetic()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
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
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // env vars はプロセス共有なので set/var を伴うテストは ENV_LOCK で逐次化する。
    static ENV_LOCK: Mutex<()> = Mutex::const_new(());

    async fn standard_metadata_mocks(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/computeMetadata/v1/project/project-id"))
            .and(header("Metadata-Flavor", "Google"))
            .respond_with(ResponseTemplate::new(200).set_body_string("cloudsql-sv"))
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path(
                "/computeMetadata/v1/instance/service-accounts/default/token",
            ))
            .and(header("Metadata-Flavor", "Google"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "tok-xyz",
                "expires_in": 3600,
            })))
            .mount(server)
            .await;
    }

    fn sm_payload(value: &str) -> Value {
        json!({"payload": {"data": BASE64_STANDARD.encode(value.as_bytes())}})
    }

    fn mock_client(server: &MockServer) -> GcpSecretsClient {
        GcpSecretsClient {
            metadata_base: server.uri(),
            sm_base: server.uri(),
        }
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
    fn valid_name_accepts_well_formed_inputs() {
        assert!(valid_name("INTERNAL_SHARED_SECRET"));
        assert!(valid_name("JWT_SECRET"));
        assert!(valid_name("a"));
        assert!(valid_name("A-B_C"));
    }

    #[test]
    fn valid_name_rejects_invalid_inputs() {
        assert!(!valid_name(""));
        assert!(!valid_name("1BAD"));
        assert!(!valid_name("BAD;NAME"));
        assert!(!valid_name("bad name"));
        assert!(!valid_name(&format!("A{}", "B".repeat(255))));
    }

    #[test]
    fn default_client_uses_production_urls() {
        let c = GcpSecretsClient::default();
        assert_eq!(c.metadata_base, "http://metadata.google.internal");
        assert_eq!(c.sm_base, "https://secretmanager.googleapis.com");
    }

    #[tokio::test]
    async fn health_endpoint_returns_ok() {
        let Json(body) = health_check().await;
        assert_eq!(body["status"], "ok");
        assert_eq!(body["service"], "alc-api");
        assert!(body["version"].is_string());
    }

    #[tokio::test]
    async fn router_constructs() {
        let _r: Router<alc_core::AppState> = router();
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_match_true_when_env_and_gcp_agree() {
        let _g = ENV_LOCK.lock().await;
        let server = MockServer::start().await;
        standard_metadata_mocks(&server).await;
        Mock::given(method("GET"))
            .and(path(
                "/v1/projects/cloudsql-sv/secrets/INTERNAL_SHARED_SECRET_TEST_FP_OK/versions/latest:access",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(sm_payload("hello")))
            .mount(&server)
            .await;
        std::env::set_var("INTERNAL_SHARED_SECRET_TEST_FP_OK", "hello");
        let (status, Json(body)) =
            secret_fingerprint_impl("INTERNAL_SHARED_SECRET_TEST_FP_OK", &mock_client(&server))
                .await;
        std::env::remove_var("INTERNAL_SHARED_SECRET_TEST_FP_OK");
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(body, json!({"match": true}));
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_match_false_when_env_and_gcp_differ() {
        let _g = ENV_LOCK.lock().await;
        let server = MockServer::start().await;
        standard_metadata_mocks(&server).await;
        Mock::given(method("GET"))
            .and(path(
                "/v1/projects/cloudsql-sv/secrets/INTERNAL_SHARED_SECRET_TEST_FP_DIFF/versions/latest:access",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(sm_payload("rotated-value")))
            .mount(&server)
            .await;
        std::env::set_var("INTERNAL_SHARED_SECRET_TEST_FP_DIFF", "old-value");
        let (status, Json(body)) =
            secret_fingerprint_impl("INTERNAL_SHARED_SECRET_TEST_FP_DIFF", &mock_client(&server))
                .await;
        std::env::remove_var("INTERNAL_SHARED_SECRET_TEST_FP_DIFF");
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(body, json!({"match": false}));
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_match_false_when_env_missing() {
        let _g = ENV_LOCK.lock().await;
        let server = MockServer::start().await;
        standard_metadata_mocks(&server).await;
        let (status, Json(body)) = secret_fingerprint_impl(
            "INTERNAL_SHARED_SECRET_NOT_SET_AT_ALL",
            &mock_client(&server),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(body, json!({"match": false}));
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_match_false_when_gcp_secret_returns_error() {
        let _g = ENV_LOCK.lock().await;
        let server = MockServer::start().await;
        standard_metadata_mocks(&server).await;
        // SM API returns 403 (permission denied) — match:false fallthrough.
        Mock::given(method("GET"))
            .and(path(
                "/v1/projects/cloudsql-sv/secrets/INTERNAL_SHARED_SECRET_TEST_FP_403/versions/latest:access",
            ))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        std::env::set_var("INTERNAL_SHARED_SECRET_TEST_FP_403", "x");
        let (status, Json(body)) =
            secret_fingerprint_impl("INTERNAL_SHARED_SECRET_TEST_FP_403", &mock_client(&server))
                .await;
        std::env::remove_var("INTERNAL_SHARED_SECRET_TEST_FP_403");
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(body, json!({"match": false}));
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_match_false_when_project_id_fetch_fails() {
        let _g = ENV_LOCK.lock().await;
        let server = MockServer::start().await;
        // metadata server returns 500 on project-id — exercises the project_id
        // Err path through the handler.
        Mock::given(method("GET"))
            .and(path("/computeMetadata/v1/project/project-id"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        std::env::set_var("INTERNAL_SHARED_SECRET_TEST_FP_PROJ_FAIL", "x");
        let (status, Json(body)) = secret_fingerprint_impl(
            "INTERNAL_SHARED_SECRET_TEST_FP_PROJ_FAIL",
            &mock_client(&server),
        )
        .await;
        std::env::remove_var("INTERNAL_SHARED_SECRET_TEST_FP_PROJ_FAIL");
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(body, json!({"match": false}));
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_match_false_when_access_token_fetch_fails() {
        let _g = ENV_LOCK.lock().await;
        let server = MockServer::start().await;
        // project-id mock OK
        Mock::given(method("GET"))
            .and(path("/computeMetadata/v1/project/project-id"))
            .respond_with(ResponseTemplate::new(200).set_body_string("cloudsql-sv"))
            .mount(&server)
            .await;
        // token endpoint returns malformed JSON (no access_token field).
        Mock::given(method("GET"))
            .and(path(
                "/computeMetadata/v1/instance/service-accounts/default/token",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"oops": "no token"})))
            .mount(&server)
            .await;
        std::env::set_var("INTERNAL_SHARED_SECRET_TEST_FP_TOK_FAIL", "x");
        let (status, Json(body)) = secret_fingerprint_impl(
            "INTERNAL_SHARED_SECRET_TEST_FP_TOK_FAIL",
            &mock_client(&server),
        )
        .await;
        std::env::remove_var("INTERNAL_SHARED_SECRET_TEST_FP_TOK_FAIL");
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(body, json!({"match": false}));
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_match_false_when_gcp_payload_is_malformed() {
        let _g = ENV_LOCK.lock().await;
        let server = MockServer::start().await;
        standard_metadata_mocks(&server).await;
        // SM returns 200 but without payload.data — exercises Err path inside fetch_secret.
        Mock::given(method("GET"))
            .and(path(
                "/v1/projects/cloudsql-sv/secrets/INTERNAL_SHARED_SECRET_TEST_FP_BAD/versions/latest:access",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"payload": {}})))
            .mount(&server)
            .await;
        std::env::set_var("INTERNAL_SHARED_SECRET_TEST_FP_BAD", "x");
        let (status, Json(body)) =
            secret_fingerprint_impl("INTERNAL_SHARED_SECRET_TEST_FP_BAD", &mock_client(&server))
                .await;
        std::env::remove_var("INTERNAL_SHARED_SECRET_TEST_FP_BAD");
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(body, json!({"match": false}));
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_match_false_when_gcp_payload_base64_is_invalid() {
        let _g = ENV_LOCK.lock().await;
        let server = MockServer::start().await;
        standard_metadata_mocks(&server).await;
        Mock::given(method("GET"))
            .and(path(
                "/v1/projects/cloudsql-sv/secrets/INTERNAL_SHARED_SECRET_TEST_FP_B64/versions/latest:access",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "payload": {"data": "not-valid-base64!@#"}
            })))
            .mount(&server)
            .await;
        std::env::set_var("INTERNAL_SHARED_SECRET_TEST_FP_B64", "x");
        let (status, Json(body)) =
            secret_fingerprint_impl("INTERNAL_SHARED_SECRET_TEST_FP_B64", &mock_client(&server))
                .await;
        std::env::remove_var("INTERNAL_SHARED_SECRET_TEST_FP_B64");
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(body, json!({"match": false}));
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_400_on_invalid_name() {
        let server = MockServer::start().await;
        let (status, Json(_body)) =
            secret_fingerprint_impl("BAD;NAME", &mock_client(&server)).await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn secret_fingerprint_returns_400_on_empty_name() {
        let server = MockServer::start().await;
        let (status, Json(_body)) = secret_fingerprint_impl("", &mock_client(&server)).await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn axum_handler_forwards_to_impl_with_default_client() {
        // 公開 handler の wrap layer (Query 抽出 + Default client) も exercise する。
        // BAD_REQUEST が返れば内部の secret_fingerprint_impl まで到達している。
        let (status, _body) = secret_fingerprint(Query(SecretFingerprintQuery {
            name: "BAD;NAME".to_string(),
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    }
}

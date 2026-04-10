use axum::{
    body::Body,
    extract::Request,
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use reqwest::Client;

use crate::auth::{extract_bearer_token, verify_jwt, AppClaims};
use crate::routes::is_public_route;

#[derive(Clone)]
pub struct ProxyState {
    pub client: Client,
    pub backend_url: String,
    pub jwt_secret: String,
    pub tenko_url: Option<String>,
    pub carins_url: Option<String>,
    pub dtako_url: Option<String>,
    pub trouble_url: Option<String>,
}

/// パスに応じてバックエンド URL を選択する
fn resolve_backend<'a>(path: &str, state: &'a ProxyState) -> &'a str {
    let api_path = path.strip_prefix("/api").unwrap_or(path);
    if api_path.starts_with("/tenko") || api_path.starts_with("/tenko-call") {
        state.tenko_url.as_deref().unwrap_or(&state.backend_url)
    } else if api_path.starts_with("/car-inspection")
        || api_path.starts_with("/files")
        || api_path.starts_with("/nfc-tags")
    {
        state.carins_url.as_deref().unwrap_or(&state.backend_url)
    } else if api_path.starts_with("/dtako-")
        || api_path.starts_with("/upload")
        || api_path.starts_with("/uploads")
        || api_path.starts_with("/recalculate")
        || api_path.starts_with("/split-csv")
        || api_path.starts_with("/drivers")
        || api_path.starts_with("/vehicles")
        || api_path.starts_with("/operations")
        || api_path.starts_with("/daily-hours")
        || api_path.starts_with("/work-times")
        || api_path.starts_with("/event-classifications")
        || api_path.starts_with("/restraint-report")
        || api_path.starts_with("/scraper/")
        || api_path.starts_with("/internal/")
    {
        state.dtako_url.as_deref().unwrap_or(&state.backend_url)
    } else if api_path.starts_with("/trouble") {
        state.trouble_url.as_deref().unwrap_or(&state.backend_url)
    } else {
        &state.backend_url
    }
}

/// リクエストを backend に転送する
pub async fn proxy_handler(
    axum::extract::State(state): axum::extract::State<ProxyState>,
    req: Request,
) -> Response {
    let (parts, body) = req.into_parts();
    let path = parts.uri.path();
    let path_and_query = parts
        .uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or(path);

    // JWT 検証 (public ルート以外)
    let claims = if is_public_route(path) {
        None
    } else {
        try_verify_jwt(&parts.headers, &state.jwt_secret)
    };

    // backend URL 構築 (パスに応じて tenko-api or backend を選択)
    let backend = resolve_backend(path, &state);
    let url = format!("{}{}", backend, path_and_query);

    // reqwest リクエスト構築
    let method = reqwest::Method::from_bytes(parts.method.as_str().as_bytes())
        .unwrap_or(reqwest::Method::GET);

    let mut builder = state.client.request(method, &url);

    // ヘッダーコピー (host 除外)
    for (name, value) in &parts.headers {
        if name == "host" {
            continue;
        }
        if let Ok(val) = reqwest::header::HeaderValue::from_bytes(value.as_bytes()) {
            if let Ok(name) = reqwest::header::HeaderName::from_bytes(name.as_ref()) {
                builder = builder.header(name, val);
            }
        }
    }

    // JWT 検証成功時にヘッダー追加
    if let Some(claims) = &claims {
        builder = inject_auth_headers(builder, claims);
    }

    // Body をストリーミング転送
    let body_stream = body.into_data_stream();
    builder = builder.body(reqwest::Body::wrap_stream(body_stream));

    // backend にリクエスト送信
    let response = match builder.send().await {
        Ok(resp) => resp,
        Err(e) => {
            if e.is_timeout() {
                tracing::error!("Backend timeout: {e}");
                return (StatusCode::GATEWAY_TIMEOUT, "gateway timeout").into_response();
            }
            tracing::error!("Backend unreachable: {e}");
            return (StatusCode::BAD_GATEWAY, "backend unavailable").into_response();
        }
    };

    // レスポンスを axum Response に変換
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);

    let mut headers = HeaderMap::new();
    for (name, value) in response.headers() {
        if let (Ok(n), Ok(v)) = (
            HeaderName::from_bytes(name.as_ref()),
            HeaderValue::from_bytes(value.as_bytes()),
        ) {
            headers.insert(n, v);
        }
    }

    let body_stream = response.bytes_stream();
    let body = Body::from_stream(body_stream);

    (status, headers, body).into_response()
}

/// Authorization ヘッダーから JWT を検証する (失敗時は None)
fn try_verify_jwt(headers: &HeaderMap, jwt_secret: &str) -> Option<AppClaims> {
    let auth_header = headers.get("authorization")?.to_str().ok()?;
    let token = extract_bearer_token(auth_header)?;
    verify_jwt(token, jwt_secret).ok()
}

/// 認証情報をヘッダーとして注入
fn inject_auth_headers(
    builder: reqwest::RequestBuilder,
    claims: &AppClaims,
) -> reqwest::RequestBuilder {
    builder
        .header("X-Tenant-ID", claims.tenant_id.to_string())
        .header("X-User-ID", claims.sub.to_string())
        .header("X-User-Email", &claims.email)
        .header("X-User-Role", &claims.role)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> ProxyState {
        ProxyState {
            client: Client::new(),
            backend_url: "http://backend:8081".to_string(),
            jwt_secret: "secret".to_string(),
            tenko_url: Some("http://tenko:8082".to_string()),
            carins_url: Some("http://carins:8083".to_string()),
            dtako_url: Some("http://dtako:8084".to_string()),
            trouble_url: Some("http://trouble:8085".to_string()),
        }
    }

    #[test]
    fn test_resolve_backend_tenko() {
        let state = test_state();
        assert_eq!(
            resolve_backend("/api/tenko/sessions", &state),
            "http://tenko:8082"
        );
        assert_eq!(
            resolve_backend("/api/tenko-call/register", &state),
            "http://tenko:8082"
        );
        assert_eq!(
            resolve_backend("/api/employees", &state),
            "http://backend:8081"
        );
    }

    #[test]
    fn test_resolve_backend_carins() {
        let state = test_state();
        assert_eq!(
            resolve_backend("/api/car-inspections/current", &state),
            "http://carins:8083"
        );
        assert_eq!(
            resolve_backend("/api/car-inspection-files/current", &state),
            "http://carins:8083"
        );
        assert_eq!(
            resolve_backend("/api/files/recent", &state),
            "http://carins:8083"
        );
        assert_eq!(
            resolve_backend("/api/nfc-tags", &state),
            "http://carins:8083"
        );
    }

    #[test]
    fn test_resolve_backend_dtako() {
        let state = test_state();
        assert_eq!(
            resolve_backend("/api/dtako-logs/current", &state),
            "http://dtako:8084"
        );
        assert_eq!(resolve_backend("/api/upload", &state), "http://dtako:8084");
        assert_eq!(resolve_backend("/api/uploads", &state), "http://dtako:8084");
        assert_eq!(
            resolve_backend("/api/recalculate", &state),
            "http://dtako:8084"
        );
        assert_eq!(
            resolve_backend("/api/split-csv/123", &state),
            "http://dtako:8084"
        );
        assert_eq!(resolve_backend("/api/drivers", &state), "http://dtako:8084");
        assert_eq!(
            resolve_backend("/api/vehicles", &state),
            "http://dtako:8084"
        );
        assert_eq!(
            resolve_backend("/api/operations", &state),
            "http://dtako:8084"
        );
        assert_eq!(
            resolve_backend("/api/daily-hours", &state),
            "http://dtako:8084"
        );
        assert_eq!(
            resolve_backend("/api/work-times", &state),
            "http://dtako:8084"
        );
        assert_eq!(
            resolve_backend("/api/event-classifications", &state),
            "http://dtako:8084"
        );
        assert_eq!(
            resolve_backend("/api/restraint-report", &state),
            "http://dtako:8084"
        );
        assert_eq!(
            resolve_backend("/api/scraper/trigger", &state),
            "http://dtako:8084"
        );
        assert_eq!(
            resolve_backend("/api/internal/pending", &state),
            "http://dtako:8084"
        );
    }

    #[test]
    fn test_resolve_backend_trouble() {
        let state = test_state();
        assert_eq!(
            resolve_backend("/api/trouble/tickets", &state),
            "http://trouble:8085"
        );
        assert_eq!(
            resolve_backend("/api/trouble/workflow/states", &state),
            "http://trouble:8085"
        );
    }

    #[test]
    fn test_resolve_backend_fallback() {
        let state = ProxyState {
            client: Client::new(),
            backend_url: "http://backend:8081".to_string(),
            jwt_secret: "secret".to_string(),
            tenko_url: None,
            carins_url: None,
            dtako_url: None,
            trouble_url: None,
        };
        assert_eq!(
            resolve_backend("/api/tenko/sessions", &state),
            "http://backend:8081"
        );
        assert_eq!(
            resolve_backend("/api/car-inspections/current", &state),
            "http://backend:8081"
        );
        assert_eq!(
            resolve_backend("/api/dtako-logs/current", &state),
            "http://backend:8081"
        );
    }

    #[test]
    fn test_try_verify_jwt_no_header() {
        let headers = HeaderMap::new();
        assert!(try_verify_jwt(&headers, "secret").is_none());
    }

    #[test]
    fn test_try_verify_jwt_invalid_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            "Bearer invalid.token.here".parse().unwrap(),
        );
        assert!(try_verify_jwt(&headers, "secret").is_none());
    }

    #[test]
    fn test_try_verify_jwt_valid_token() {
        use chrono::{Duration, Utc};
        use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};

        let secret = "test-secret-key-256-bits-long!!!";
        let now = Utc::now();
        let claims = AppClaims {
            sub: uuid::Uuid::new_v4(),
            email: "test@example.com".to_string(),
            name: "Test".to_string(),
            tenant_id: uuid::Uuid::new_v4(),
            role: "admin".to_string(),
            org_slug: None,
            iat: now.timestamp(),
            exp: (now + Duration::hours(1)).timestamp(),
        };
        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert("authorization", format!("Bearer {token}").parse().unwrap());

        let result = try_verify_jwt(&headers, secret);
        assert!(result.is_some());
        assert_eq!(result.unwrap().email, "test@example.com");
    }
}

use std::env;

pub struct Config {
    pub port: u16,
    pub backend_url: String,
    /// auth-worker の base URL (introspect 委譲先、Refs #479 PR-2)。
    pub auth_worker_url: String,
    /// auth-worker `/auth/introspect` の server-to-server 認証 secret。
    pub introspect_shared_secret: String,
    /// tenko-api の URL。未設定時は backend_url にフォールバック。
    pub tenko_url: Option<String>,
    /// carins-api の URL。未設定時は backend_url にフォールバック。
    pub carins_url: Option<String>,
    /// dtako-api の URL。未設定時は backend_url にフォールバック。
    pub dtako_url: Option<String>,
    /// trouble-api の URL。未設定時は backend_url にフォールバック。
    pub trouble_url: Option<String>,
    /// alc-camera-api の URL。未設定時は backend_url にフォールバック。
    pub camera_url: Option<String>,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            port: env::var("PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(8080),
            backend_url: env::var("BACKEND_URL").expect("BACKEND_URL is required"),
            auth_worker_url: env::var("AUTH_WORKER_URL").expect("AUTH_WORKER_URL is required"),
            introspect_shared_secret: env::var("INTERNAL_SHARED_SECRET")
                .expect("INTERNAL_SHARED_SECRET is required"),
            tenko_url: env::var("TENKO_API_URL").ok(),
            carins_url: env::var("CARINS_API_URL").ok(),
            dtako_url: env::var("DTAKO_API_URL").ok(),
            trouble_url: env::var("TROUBLE_API_URL").ok(),
            camera_url: env::var("CAMERA_API_URL").ok(),
        }
    }
}

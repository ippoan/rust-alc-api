use std::env;

pub struct Config {
    pub port: u16,
    pub backend_url: String,
    pub jwt_secret: String,
    /// tenko-api の URL。未設定時は backend_url にフォールバック。
    pub tenko_url: Option<String>,
    /// carins-api の URL。未設定時は backend_url にフォールバック。
    pub carins_url: Option<String>,
    /// dtako-api の URL。未設定時は backend_url にフォールバック。
    pub dtako_url: Option<String>,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            port: env::var("PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(8080),
            backend_url: env::var("BACKEND_URL").expect("BACKEND_URL is required"),
            jwt_secret: env::var("JWT_SECRET").expect("JWT_SECRET is required"),
            tenko_url: env::var("TENKO_API_URL").ok(),
            carins_url: env::var("CARINS_API_URL").ok(),
            dtako_url: env::var("DTAKO_API_URL").ok(),
        }
    }
}

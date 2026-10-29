#[cfg(test)]
#[macro_use]
mod test_macros;

// Auth modules (formerly alc-auth crate)
pub mod auth_google;
pub mod auth_jwt;
pub mod auth_lineworks;
pub mod auth_middleware;

pub mod api_error;
pub mod constant_time;
pub mod device_dev;
pub mod middleware;
pub mod models;
pub mod repo;
pub mod repository;
pub mod serde_helpers;
pub mod storage;
pub mod tenant;

use std::sync::Arc;

use repository::{AuthRepository, BotAdminRepository};
use storage::StorageBackend;

#[derive(Clone)]
pub struct AppState {
    pub pool: Option<sqlx::PgPool>,
    pub auth: Arc<dyn AuthRepository>,
    pub bot_admin: Arc<dyn BotAdminRepository>,
    pub storage: Arc<dyn StorageBackend>,
}

impl AppState {
    /// pool が必要な統合テスト・本番コード用。None なら panic。
    pub fn pool(&self) -> &sqlx::PgPool {
        self.pool.as_ref().expect("PgPool is required but not set")
    }
}

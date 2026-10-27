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
pub mod realtime_bus;
pub mod redact_broadcast;
pub mod repo;
pub mod repository;
pub mod serde_helpers;
pub mod storage;
pub mod tenant;
pub mod webhook;

use std::sync::Arc;

use repository::{
    AuthRepository, BotAdminRepository, LineworksChannelsRepository, NotifyDeliveryRepository,
    NotifyDocumentRepository, NotifyRecipientRepository,
};
use storage::StorageBackend;

#[derive(Clone)]
pub struct AppState {
    pub pool: Option<sqlx::PgPool>,
    pub auth: Arc<dyn AuthRepository>,
    pub bot_admin: Arc<dyn BotAdminRepository>,
    pub storage: Arc<dyn StorageBackend>,
    pub webhook: Option<Arc<dyn webhook::WebhookService>>,
    pub notify_recipients: Arc<dyn NotifyRecipientRepository>,
    pub notify_documents: Arc<dyn NotifyDocumentRepository>,
    pub notify_deliveries: Arc<dyn NotifyDeliveryRepository>,
    pub lineworks_channels: Arc<dyn LineworksChannelsRepository>,
    pub notify_storage: Option<Arc<dyn StorageBackend>>,
    /// `RedactBroadcaster::from_env()` で env vars (`NOTIFY_REDACT_BROADCAST_URL` /
    /// `NOTIFY_REDACT_BROADCAST_SECRET`) が揃った時のみ Some。background_redaction.rs
    /// が terminal 状態で呼び、Cloudflare Worker (notify-realtime-bus) の DO 経由で
    /// admin ブラウザに WS push する。未設定なら no-op (Phase 3 デプロイ前は空でも安全)。
    pub redact_broadcaster: Option<Arc<redact_broadcast::RedactBroadcaster>>,
    /// 任意の Serialize ペイロードを同 notify-realtime-bus Worker に流す汎用クライアント。
    /// `RedactBroadcaster` と同じ env vars / 同じ Worker / 同じ secret を共有するが、
    /// payload 型を選ばないので redact 以外の async job 完了通知 (Y時間 export 等)
    /// にも使える。env vars 未設定なら None で no-op (POST /jobs 系は 503 返却で
    /// 検知すること、silent に compute しないため)。
    pub realtime_bus: Option<Arc<realtime_bus::RealtimeBus>>,
}

impl AppState {
    /// pool が必要な統合テスト・本番コード用。None なら panic。
    pub fn pool(&self) -> &sqlx::PgPool {
        self.pool.as_ref().expect("PgPool is required but not set")
    }
}

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use rust_alc_api::AppState;

use super::*;
use crate::common::mock_storage::MockStorage;

/// DB 不要の mock AppState を構築。
/// pool: None — mock repo が全ハンドラを処理するため DB 接続不要。
/// テスト側で `state.xxx` の `fail_next` を設定して DB エラー注入可能。
pub fn setup_mock_app_state() -> AppState {
    // tracing 初期化 (1回だけ)
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    let storage: Arc<dyn rust_alc_api::storage::StorageBackend> =
        Arc::new(MockStorage::new("test-bucket"));

    AppState {
        pool: None,
        auth: Arc::new(MockAuthRepository::default()),
        bot_admin: Arc::new(MockBotAdminRepository::default()),
        storage,
        notify_recipients: Arc::new(MockNotifyRecipientRepository::default()),
        notify_documents: Arc::new(MockNotifyDocumentRepository::default()),
        notify_deliveries: Arc::new(MockNotifyDeliveryRepository::default()),
        lineworks_channels: Arc::new(MockLineworksChannelsRepository::default()),
        notify_storage: None,
        redact_broadcaster: None,
        realtime_bus: None,
    }
}

/// mock 用 spawn wrapper。
pub async fn spawn_mock_server(state: AppState) -> String {
    crate::common::spawn_test_server(state).await
}

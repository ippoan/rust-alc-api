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
        webhook: None,
    }
}

/// tenko ドメインの mock TenkoState (Refs #513)。tenko 系 mock test は
/// 差し替えたい field を上書きしてから `spawn_test_server_with_tenko` に渡す。
pub fn setup_mock_tenko_state() -> alc_tenko::TenkoState {
    alc_tenko::TenkoState {
        tenko_call: Arc::new(MockTenkoCallRepository::default()),
        tenko_records: Arc::new(MockTenkoRecordsRepository::default()),
        tenko_schedules: Arc::new(MockTenkoSchedulesRepository::default()),
        tenko_sessions: Arc::new(MockTenkoSessionRepository::default()),
        tenko_webhooks: Arc::new(MockTenkoWebhooksRepository::default()),
        daily_health: Arc::new(MockDailyHealthRepository::default()),
        health_baselines: Arc::new(MockHealthBaselinesRepository::default()),
        equipment_failures: Arc::new(MockEquipmentFailuresRepository::default()),
        driver_info: Arc::new(MockDriverInfoRepository::default()),
        webhook: None,
    }
}

/// mock 用 spawn wrapper: デフォルトの mock TenkoState を合成して起動する。
pub async fn spawn_mock_server(state: AppState) -> String {
    crate::common::spawn_test_server_with_states(state, setup_mock_tenko_state()).await
}

/// tenko_state を差し替えたい mock テスト用。
pub async fn spawn_mock_server_with_tenko(
    state: AppState,
    tenko_state: alc_tenko::TenkoState,
) -> String {
    crate::common::spawn_test_server_with_states(state, tenko_state).await
}

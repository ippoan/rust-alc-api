pub mod daily_health;
pub mod equipment_failures;
pub mod health_baselines;
pub mod repo;
pub mod tenko_call;
pub mod tenko_records;
pub mod tenko_schedules;
pub mod tenko_sessions;
pub mod tenko_webhooks;

use std::sync::Arc;

use alc_core::repository::{
    DailyHealthRepository, EquipmentFailuresRepository, HealthBaselinesRepository,
    TenkoCallRepository, TenkoRecordsRepository, TenkoSchedulesRepository, TenkoSessionRepository,
    TenkoWebhooksRepository,
};
use alc_core::webhook::WebhookService;

/// tenko-api 用の最小 State。
/// モノリスでは `FromRef<AppState>` 経由で自動変換される。
#[derive(Clone)]
pub struct TenkoState {
    pub tenko_call: Arc<dyn TenkoCallRepository>,
    pub tenko_records: Arc<dyn TenkoRecordsRepository>,
    pub tenko_schedules: Arc<dyn TenkoSchedulesRepository>,
    pub tenko_sessions: Arc<dyn TenkoSessionRepository>,
    pub tenko_webhooks: Arc<dyn TenkoWebhooksRepository>,
    pub daily_health: Arc<dyn DailyHealthRepository>,
    pub health_baselines: Arc<dyn HealthBaselinesRepository>,
    pub equipment_failures: Arc<dyn EquipmentFailuresRepository>,
    pub webhook: Option<Arc<dyn WebhookService>>,
}

impl axum::extract::FromRef<alc_core::AppState> for TenkoState {
    fn from_ref(state: &alc_core::AppState) -> Self {
        Self {
            tenko_call: state.tenko_call.clone(),
            tenko_records: state.tenko_records.clone(),
            tenko_schedules: state.tenko_schedules.clone(),
            tenko_sessions: state.tenko_sessions.clone(),
            tenko_webhooks: state.tenko_webhooks.clone(),
            daily_health: state.daily_health.clone(),
            health_baselines: state.health_baselines.clone(),
            equipment_failures: state.equipment_failures.clone(),
            webhook: state.webhook.clone(),
        }
    }
}

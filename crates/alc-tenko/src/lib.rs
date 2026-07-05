pub mod daily_health;
pub mod driver_info;
pub mod equipment_failures;
pub mod health_baselines;
pub mod models;
pub mod overdue;
pub mod repo;
pub mod repository;
pub mod tenko_call;
pub mod tenko_records;
pub mod tenko_schedules;
pub mod tenko_sessions;
pub mod tenko_webhooks;

use std::sync::Arc;

use crate::repository::{
    DailyHealthRepository, DriverInfoRepository, EquipmentFailuresRepository,
    HealthBaselinesRepository, TenkoCallRepository, TenkoRecordsRepository,
    TenkoSchedulesRepository, TenkoSessionRepository, TenkoWebhooksRepository,
};
use alc_core::webhook::WebhookService;

/// tenko ドメインの State。tenko-api はこれをそのまま使い、
/// モノリスは tenko route 群を `.with_state(TenkoState { .. })` でマウントする
/// (旧 `FromRef<AppState>` 変換は AppState から tenko field を撤去した際に廃止、Refs #513)。
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
    pub driver_info: Arc<dyn DriverInfoRepository>,
    pub webhook: Option<Arc<dyn WebhookService>>,
}

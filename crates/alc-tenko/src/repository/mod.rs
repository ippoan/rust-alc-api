// tenko ドメインの repository trait 群 (alc-core から移設、Refs #513)。
// Pg 実装は crate::repo、モックはテスト側 (tests/mock_helpers) が持つ。

pub mod daily_health;
pub mod driver_info;
pub mod equipment_failures;
pub mod health_baselines;
pub mod tenko_call;
pub mod tenko_records;
pub mod tenko_schedules;
pub mod tenko_sessions;
pub mod tenko_webhooks;

pub use daily_health::DailyHealthRepository;
pub use driver_info::DriverInfoRepository;
pub use equipment_failures::EquipmentFailuresRepository;
pub use health_baselines::HealthBaselinesRepository;
pub use tenko_call::TenkoCallRepository;
pub use tenko_records::TenkoRecordsRepository;
pub use tenko_schedules::TenkoSchedulesRepository;
pub use tenko_sessions::TenkoSessionRepository;
pub use tenko_webhooks::TenkoWebhooksRepository;

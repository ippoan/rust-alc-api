#[macro_use]
#[path = "../common/mod.rs"]
mod common;

#[path = "../mock_helpers/mod.rs"]
mod mock_helpers;

#[path = "../mock_tests/mock_daily_health_test.rs"]
mod mock_daily_health_test;
#[path = "../mock_tests/mock_equipment_failures_test.rs"]
mod mock_equipment_failures_test;
#[path = "../mock_tests/mock_health_baselines_test.rs"]
mod mock_health_baselines_test;
#[path = "../mock_tests/mock_tenko_call_test.rs"]
mod mock_tenko_call_test;
#[path = "../mock_tests/mock_tenko_records_test.rs"]
mod mock_tenko_records_test;
#[path = "../mock_tests/mock_tenko_schedules_test.rs"]
mod mock_tenko_schedules_test;
#[path = "../mock_tests/mock_tenko_sessions_test.rs"]
mod mock_tenko_sessions_test;
#[path = "../mock_tests/mock_tenko_webhooks_test.rs"]
mod mock_tenko_webhooks_test;

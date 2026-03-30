//! Mock テスト統合バイナリ (DB 不要)
//!
//! 37 個の mock テストを 1 バイナリにまとめてリンク時間を削減する。
//! CI: `cargo llvm-cov --test mock_tests --text`

#[macro_use]
#[path = "../common/mod.rs"]
mod common;

#[path = "../mock_helpers/mod.rs"]
mod mock_helpers;

mod mock_auth_test;
mod mock_bot_admin_test;
mod mock_car_inspection_files_test;
mod mock_car_inspections_test;
mod mock_carins_files_test;
mod mock_carrying_items_test;
mod mock_communication_items_test;
mod mock_daily_health_test;
mod mock_devices_test;
mod mock_driver_info_test;
mod mock_dtako_csv_proxy_test;
mod mock_dtako_daily_hours_test;
mod mock_dtako_drivers_test;
mod mock_dtako_event_classifications_test;
mod mock_dtako_operations_test;
mod mock_dtako_restraint_report_pdf_test;
mod mock_dtako_restraint_report_test;
mod mock_dtako_scraper_test;
mod mock_dtako_upload_test;
mod mock_dtako_vehicles_test;
mod mock_dtako_work_times_test;
mod mock_employees_test;
mod mock_equipment_failures_test;
mod mock_guidance_records_test;
mod mock_health_baselines_test;
mod mock_health_test;
mod mock_measurements_test;
mod mock_nfc_tags_test;
mod mock_sso_admin_test;
mod mock_tenant_users_test;
mod mock_tenko_call_test;
mod mock_tenko_records_test;
mod mock_tenko_schedules_test;
mod mock_tenko_sessions_test;
mod mock_tenko_webhooks_test;
mod mock_timecard_test;
mod mock_upload_test;

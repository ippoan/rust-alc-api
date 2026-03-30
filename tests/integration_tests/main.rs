//! Integration テスト統合バイナリ (DB 必要)
//!
//! CI: `cargo test --test integration_tests`

#[macro_use]
#[path = "../common/mod.rs"]
mod common;

mod admin_coverage;
mod admin_test;
mod auth_coverage;
mod auth_test;
mod carins_test;
mod daily_health;
mod devices_test;
mod dtako_csv_proxy;
mod dtako_daily_hours;
mod dtako_scraper;
mod dtako_test;
mod employees_coverage;
mod employees_test;
mod google_auth;
mod health_test;
mod measurements_test;
mod misc;
mod misc_routes_test;
mod sso_coverage;
mod tenko_call_coverage;
mod tenko_test;
mod timecard_coverage;

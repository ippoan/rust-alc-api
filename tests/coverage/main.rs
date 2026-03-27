/// カバレッジ 100% 達成用テスト
///
/// cargo test では全テスト #[ignore] (スキップ)、cargo llvm-cov では実行。
/// 通常テストの実行時間を短縮しつつ、カバレッジ計測時のみ
/// エラーパス・エッジケースを網羅する。
#[macro_use]
#[path = "../common/mod.rs"]
mod common;

mod auth_coverage;
mod daily_health;
mod dtako_csv_proxy;
mod dtako_daily_hours;
mod dtako_scraper;
mod google_auth;
mod misc;
mod tenko_call_coverage;

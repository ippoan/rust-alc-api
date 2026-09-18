//! maintenance ドメインの DB 不要 (mock) テストをまとめる binary
//! (`tests/mock_trouble/main.rs` が手本、Refs ippoan/rust-alc-api#651)。
//!
//! 車両マスタ API (#c651-2) の検証は実 DB が要る (carins 照合・RLS・partial unique)
//! ため `tests/maintenance_test.rs` (DB 統合テスト) 側にある。写真添付 (#c651-6) の
//! handler 分岐 (404/503/500) は DB 不要なのでここに足す。

#[macro_use]
#[path = "../common/mod.rs"]
mod common;

#[path = "../mock_helpers/mod.rs"]
mod mock_helpers;

#[path = "../mock_tests/mock_maintenance_files_test.rs"]
mod mock_maintenance_files_test;

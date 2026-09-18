//! maintenance ドメインの DB 不要 (mock) テストをまとめる binary
//! (`tests/mock_trouble/main.rs` が手本、Refs ippoan/rust-alc-api#651)。
//!
//! このタスク (#c651-2、車両マスタ API) の検証は実 DB が要る (carins 照合・RLS・
//! partial unique) ため `tests/maintenance_test.rs` (DB 統合テスト) 側に置いた。
//! ここは後続タスク (カテゴリ / 整備記録 / 写真) が DB 不要な mock テストを足す
//! ときの受け皿として空のまま用意しておく。

#[macro_use]
#[path = "../common/mod.rs"]
mod common;

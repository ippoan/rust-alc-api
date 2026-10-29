//! Mock Repository 基盤
//! DB 不要でルートハンドラをテストするための mock 実装群。
//!
//! 各 mock struct は `fail_next: AtomicBool` を持ち、
//! true にすると次のメソッド呼び出しで sqlx::Error を返す。

// 複数の test binary から path include される共有モジュール。
// binary ごとに使う mock / helper が違うので per-binary の未使用警告を許容する。
#![allow(dead_code, unused_imports)]

#[macro_use]
mod repos_a;
pub mod app_state;

pub use app_state::*;
pub use repos_a::*;

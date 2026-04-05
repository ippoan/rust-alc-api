//! Mock Repository 基盤
//! DB 不要でルートハンドラをテストするための mock 実装群。
//!
//! 各 mock struct は `fail_next: AtomicBool` を持ち、
//! true にすると次のメソッド呼び出しで sqlx::Error を返す。

#[macro_use]
mod repos_a;
#[macro_use]
mod repos_b;
#[macro_use]
mod repos_c;
#[macro_use]
mod repos_d;
pub mod app_state;
pub mod webhook;

pub use app_state::*;
pub use repos_a::*;
pub use repos_b::*;
pub use repos_c::*;
pub use repos_d::*;

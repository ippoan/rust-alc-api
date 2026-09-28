//! wasm32 (Cloudflare Workers) でもビルドできる `alc-core` の共有部分。
//!
//! sqlx / reqwest / ring / tokio に依存する部分は `alc-core` に残す。
//! `alc-core` は本 crate の項目を元のパスのまま re-export する。

pub mod api_error;
pub mod db_error;
pub mod tenant_header;
pub mod types;

pub use db_error::DbError;
pub use tenant_header::require_tenant_header;
pub use types::{AuthUser, TenantId};

#[cfg(test)]
#[macro_use]
mod test_macros;

pub mod archive;
pub mod auth;
pub use alc_compare as compare;
pub use alc_csv_parser as csv_parser;
pub mod db;
pub mod fcm;
pub mod middleware;
pub mod routes;
pub mod storage;
pub mod webhook;

pub use alc_core::AppState;

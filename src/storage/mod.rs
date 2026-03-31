pub mod gcs;
pub mod r2;

pub use alc_core::storage::{StorageBackend, StorageError};
pub use gcs::GcsBackend;
pub use r2::R2Backend;

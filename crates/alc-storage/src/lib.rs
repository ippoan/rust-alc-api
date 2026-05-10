pub mod gcs;
pub mod http_proxy;
pub mod r2;

pub use gcs::GcsBackend;
pub use http_proxy::HttpProxyBackend;
pub use r2::R2Backend;

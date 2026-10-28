// Re-export traits from alc-core
pub use alc_core::repository::{
    AuthRepository, BotAdminRepository, NotifyDeliveryRepository, NotifyDocumentRepository,
};

// Re-export TenantConn from alc-core
pub use alc_core::tenant::TenantConn;

// Re-export submodules for backward compatibility (tests use repository::xxx::TypeName)
pub use alc_core::repo::auth;
pub use alc_misc::repo::bot_admin;

// Re-export notify submodules and Pg implementations
pub use alc_core::repository::{notify_deliveries, notify_documents};
pub use alc_notify::repo::{PgNotifyDeliveryRepository, PgNotifyDocumentRepository};

// Re-export Pg implementations
pub use alc_core::repo::PgAuthRepository;
pub use alc_misc::repo::PgBotAdminRepository;

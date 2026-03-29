#[cfg(test)]
#[macro_use]
mod test_macros;

pub mod auth;
pub mod compare;
pub mod csv_parser;
pub mod db;
pub mod fcm;
pub mod middleware;
pub mod routes;
pub mod storage;
pub mod webhook;

use std::sync::Arc;

use db::repository::{
    CommunicationItemsRepository, EmployeeRepository, NfcTagRepository, TenkoCallRepository,
    TimecardRepository,
};
use storage::StorageBackend;

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub employees: Arc<dyn EmployeeRepository>,
    pub communication_items: Arc<dyn CommunicationItemsRepository>,
    pub timecard: Arc<dyn TimecardRepository>,
    pub tenko_call: Arc<dyn TenkoCallRepository>,
    pub nfc_tags: Arc<dyn NfcTagRepository>,
    pub storage: Arc<dyn StorageBackend>,
    pub carins_storage: Option<Arc<dyn StorageBackend>>,
    pub dtako_storage: Option<Arc<dyn StorageBackend>>,
    pub fcm: Option<Arc<dyn fcm::FcmSenderTrait>>,
}

pub mod categories;
pub mod cloud_tasks;
pub mod comments;
pub mod files;
pub mod lineworks_members;
pub mod notifications;
pub mod notifier;
pub mod offices;
pub mod progress_statuses;
pub mod repo;
pub mod schedules;
pub mod task_types;
pub mod tasks;
pub mod tickets;
pub mod workflow;

pub const DEFAULT_CATEGORIES: &[&str] = &[
    "苦情・トラブル",
    "貨物事故",
    "被害事故",
    "対物事故(他損)",
    "対物事故(自損)",
    "人身事故",
    "その他",
];

pub const DEFAULT_TASK_TYPES: &[&str] = &[
    "レッカー対応",
    "修理手配",
    "保険対応",
    "示談交渉",
    "処分決定",
    "再発防止策",
    "現場確認",
    "その他",
];

use std::sync::Arc;

use alc_core::repository::{
    EmployeeRepository, TroubleActivityFilesRepository, TroubleCategoriesRepository,
    TroubleCommentsRepository, TroubleFilesRepository, TroubleNotificationPrefsRepository,
    TroubleOfficesRepository, TroubleProgressStatusesRepository, TroubleSchedulesRepository,
    TroubleTaskActivitiesRepository, TroubleTaskTypesRepository, TroubleTasksRepository,
    TroubleTicketsRepository, TroubleWorkflowRepository,
};
use alc_core::storage::StorageBackend;
use alc_core::webhook::WebhookService;

use crate::cloud_tasks::CloudTasksClient;
use crate::notifier::TroubleNotifier;

/// trouble 用の最小 State。
/// モノリスでは `FromRef<AppState>` 経由で自動変換される。
#[derive(Clone)]
pub struct TroubleState {
    pub trouble_tickets: Arc<dyn TroubleTicketsRepository>,
    pub trouble_files: Arc<dyn TroubleFilesRepository>,
    pub trouble_workflow: Arc<dyn TroubleWorkflowRepository>,
    pub trouble_comments: Arc<dyn TroubleCommentsRepository>,
    pub trouble_categories: Arc<dyn TroubleCategoriesRepository>,
    pub trouble_offices: Arc<dyn TroubleOfficesRepository>,
    pub trouble_progress_statuses: Arc<dyn TroubleProgressStatusesRepository>,
    pub trouble_notification_prefs: Arc<dyn TroubleNotificationPrefsRepository>,
    pub trouble_schedules: Arc<dyn TroubleSchedulesRepository>,
    pub trouble_tasks: Arc<dyn TroubleTasksRepository>,
    pub trouble_task_activities: Arc<dyn TroubleTaskActivitiesRepository>,
    pub trouble_activity_files: Arc<dyn TroubleActivityFilesRepository>,
    pub trouble_task_types: Arc<dyn TroubleTaskTypesRepository>,
    pub trouble_storage: Option<Arc<dyn StorageBackend>>,
    pub webhook: Option<Arc<dyn WebhookService>>,
    pub cloud_tasks: Option<Arc<dyn CloudTasksClient>>,
    pub notifier: Option<Arc<dyn TroubleNotifier>>,
    pub employees: Option<Arc<dyn EmployeeRepository>>,
}

impl axum::extract::FromRef<alc_core::AppState> for TroubleState {
    fn from_ref(state: &alc_core::AppState) -> Self {
        Self {
            trouble_tickets: state.trouble_tickets.clone(),
            trouble_files: state.trouble_files.clone(),
            trouble_workflow: state.trouble_workflow.clone(),
            trouble_comments: state.trouble_comments.clone(),
            trouble_categories: state.trouble_categories.clone(),
            trouble_offices: state.trouble_offices.clone(),
            trouble_progress_statuses: state.trouble_progress_statuses.clone(),
            trouble_notification_prefs: state.trouble_notification_prefs.clone(),
            trouble_schedules: state.trouble_schedules.clone(),
            trouble_tasks: state.trouble_tasks.clone(),
            trouble_task_activities: state.trouble_task_activities.clone(),
            trouble_activity_files: state.trouble_activity_files.clone(),
            trouble_task_types: state.trouble_task_types.clone(),
            trouble_storage: state.trouble_storage.clone(),
            webhook: state.webhook.clone(),
            cloud_tasks: None,
            notifier: None,
            employees: Some(state.employees.clone()),
        }
    }
}

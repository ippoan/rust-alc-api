//! trouble ドメインの repository trait (alc-core::repository から移設、Refs #513 Phase B)。
//! Pg 実装は `crate::repo`。

pub mod trouble_categories;
pub mod trouble_field_layouts;
pub mod trouble_files;
pub mod trouble_notification_prefs;
pub mod trouble_offices;
pub mod trouble_progress_statuses;
pub mod trouble_schedules;
pub mod trouble_task_statuses;
pub mod trouble_task_types;
pub mod trouble_tasks;
pub mod trouble_tickets;
pub mod trouble_workflow;

pub use trouble_categories::TroubleCategoriesRepository;
pub use trouble_field_layouts::TroubleFieldLayoutsRepository;
pub use trouble_files::TroubleFilesRepository;
pub use trouble_notification_prefs::TroubleNotificationPrefsRepository;
pub use trouble_offices::TroubleOfficesRepository;
pub use trouble_progress_statuses::TroubleProgressStatusesRepository;
pub use trouble_schedules::TroubleSchedulesRepository;
pub use trouble_task_statuses::TroubleTaskStatusesRepository;
pub use trouble_task_types::TroubleTaskTypesRepository;
pub use trouble_tasks::{TroubleTasksFilter, TroubleTasksRepository, TroubleTasksSortBy};
pub use trouble_tickets::TroubleTicketsRepository;
pub use trouble_workflow::TroubleWorkflowRepository;

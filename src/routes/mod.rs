// Re-export route modules from domain crates
pub use alc_carins::car_inspection_files;
pub use alc_carins::car_inspections;
pub use alc_carins::carins_files;
pub use alc_carins::nfc_tags;
pub use alc_devices::devices;
pub use alc_dtako::dtako_csv_proxy;
pub use alc_dtako::dtako_daily_hours;
pub use alc_dtako::dtako_drivers;
pub use alc_dtako::dtako_event_classifications;
pub use alc_dtako::dtako_operations;
pub use alc_dtako::dtako_restraint_report;
pub use alc_dtako::dtako_restraint_report_pdf;
pub use alc_dtako::dtako_scraper;
pub use alc_dtako::dtako_upload;
pub use alc_dtako::dtako_vehicles;
pub use alc_dtako::dtako_work_times;
pub use alc_misc::auth;
pub use alc_misc::bot_admin;
pub use alc_misc::carrying_items;
pub use alc_misc::communication_items;
pub use alc_misc::driver_info;
pub use alc_misc::employees;
pub use alc_misc::guidance_records;
pub use alc_misc::health;
pub use alc_misc::measurements;
pub use alc_misc::sso_admin;
pub use alc_misc::staging;
pub use alc_misc::tenant_users;
pub use alc_misc::timecard;
pub use alc_misc::upload;
pub use alc_tenko::daily_health;
pub use alc_tenko::equipment_failures;
pub use alc_tenko::health_baselines;
pub use alc_tenko::tenko_call;
pub use alc_tenko::tenko_records;
pub use alc_tenko::tenko_schedules;
pub use alc_tenko::tenko_sessions;
pub use alc_tenko::tenko_webhooks;

use axum::{middleware as axum_middleware, Router};

use crate::middleware::auth::{require_jwt, require_tenant};
use crate::AppState;

pub fn router() -> Router<AppState> {
    // JWT 必須ルート
    let jwt_protected = Router::new()
        .merge(auth::protected_router())
        .merge(sso_admin::router())
        .merge(bot_admin::router())
        .merge(tenant_users::router())
        .layer(axum_middleware::from_fn(require_jwt));

    // テナント対応ルート (JWT or X-Tenant-ID)
    let tenant_protected = Router::new()
        .merge(employees::tenant_router())
        .merge(measurements::router())
        .merge(measurements::tenant_router())
        .merge(upload::tenant_router())
        .merge(tenko_schedules::tenant_router())
        .merge(tenko_sessions::tenant_router())
        .merge(tenko_records::tenant_router())
        .merge(health_baselines::tenant_router())
        .merge(equipment_failures::tenant_router())
        .merge(tenko_webhooks::tenant_router())
        .merge(tenko_call::tenant_router())
        .merge(timecard::tenant_router())
        .merge(devices::tenant_router())
        .merge(car_inspections::tenant_router())
        .merge(car_inspection_files::tenant_router())
        .merge(carins_files::tenant_router())
        .merge(nfc_tags::tenant_router())
        .merge(carrying_items::tenant_router())
        .merge(communication_items::tenant_router())
        .merge(daily_health::tenant_router())
        .merge(driver_info::tenant_router())
        .merge(guidance_records::tenant_router())
        .merge(dtako_csv_proxy::tenant_router())
        .merge(dtako_drivers::tenant_router())
        .merge(dtako_operations::tenant_router())
        .merge(dtako_restraint_report::tenant_router())
        .merge(dtako_restraint_report_pdf::tenant_router())
        .merge(dtako_scraper::tenant_router())
        .merge(dtako_work_times::tenant_router())
        .merge(dtako_daily_hours::tenant_router())
        .merge(dtako_upload::tenant_router())
        .merge(dtako_vehicles::tenant_router())
        .merge(dtako_event_classifications::tenant_router())
        .layer(axum_middleware::from_fn(require_tenant));

    // 公開ルート (認証不要)
    let public_routes = Router::new()
        .merge(health::router())
        .merge(auth::public_router())
        .merge(tenko_call::public_router())
        .merge(devices::public_router())
        .merge(staging::router());

    Router::new()
        .merge(public_routes)
        .merge(jwt_protected)
        .merge(tenant_protected)
}

pub mod auth;
pub mod car_inspection_files;
pub mod car_inspections;
pub mod carins_files;
pub mod devices;
pub mod employees;
pub mod equipment_failures;
pub mod health_baselines;
pub mod measurements;
pub mod nfc_tags;
pub mod tenko_records;
pub mod tenko_schedules;
pub mod tenko_sessions;
pub mod tenko_webhooks;
pub mod tenko_call;
pub mod timecard;
pub mod upload;
pub mod sso_admin;
pub mod bot_admin;
pub mod tenant_users;
pub mod carrying_items;
pub mod communication_items;
pub mod daily_health;
pub mod driver_info;
pub mod guidance_records;
pub mod dtako_drivers;
pub mod dtako_csv_proxy;
pub mod dtako_operations;
pub mod dtako_restraint_report;
pub mod dtako_restraint_report_pdf;
pub mod dtako_scraper;
pub mod dtako_work_times;
pub mod dtako_daily_hours;
pub mod dtako_upload;
pub mod dtako_vehicles;
pub mod dtako_event_classifications;

use axum::{middleware as axum_middleware, Router};

use crate::AppState;
use crate::middleware::auth::{require_jwt, require_tenant};

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
        .merge(auth::public_router())
        .merge(tenko_call::public_router())
        .merge(devices::public_router());

    Router::new()
        .merge(public_routes)
        .merge(jwt_protected)
        .merge(tenant_protected)
}

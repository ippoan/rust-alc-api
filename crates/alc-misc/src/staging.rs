use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::AppState;

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/staging/export", get(export_handler))
        .route("/staging/import", post(import_handler))
}

// ---------------------------------------------------------------------------
// Environment guard
// ---------------------------------------------------------------------------

fn is_staging_mode() -> bool {
    std::env::var("STAGING_MODE")
        .map(|v| v == "true")
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Data structures (staging-specific — includes fields skipped in normal API)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct StagingExportData {
    pub version: u32,
    pub exported_at: String,
    pub tenant_id: String,
    pub data: StagingData,
}

#[derive(Serialize, Deserialize)]
pub struct StagingData {
    pub tenant: StagingTenant,
    pub users: Vec<StagingUser>,
    pub employees: Vec<StagingEmployee>,
    pub devices: Vec<StagingDevice>,
    pub tenko_schedules: Vec<StagingTenkoSchedule>,
    pub webhook_configs: Vec<StagingWebhookConfig>,
    pub tenant_allowed_emails: Vec<StagingTenantAllowedEmail>,
    pub sso_provider_configs: Vec<StagingSsoProviderConfig>,
    pub tenko_call_numbers: Vec<StagingTenkoCallNumber>,
    pub tenko_call_drivers: Vec<StagingTenkoCallDriver>,
}

#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub struct StagingTenant {
    pub id: Uuid,
    pub name: String,
    pub slug: Option<String>,
    pub email_domain: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub struct StagingUser {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub google_sub: Option<String>,
    pub lineworks_id: Option<String>,
    pub email: String,
    pub name: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub struct StagingEmployee {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub code: Option<String>,
    pub nfc_id: Option<String>,
    pub name: String,
    pub face_photo_url: Option<String>,
    pub face_embedding: Option<Vec<f64>>,
    pub face_embedding_at: Option<DateTime<Utc>>,
    pub face_model_version: Option<String>,
    pub face_approval_status: String,
    pub face_approved_by: Option<Uuid>,
    pub face_approved_at: Option<DateTime<Utc>>,
    pub license_issue_date: Option<NaiveDate>,
    pub license_expiry_date: Option<NaiveDate>,
    pub role: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub struct StagingDevice {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub device_name: String,
    pub device_type: String,
    pub phone_number: Option<String>,
    pub user_id: Option<Uuid>,
    pub status: String,
    pub approved_by: Option<Uuid>,
    pub approved_at: Option<DateTime<Utc>>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub call_enabled: bool,
    pub call_schedule: Option<serde_json::Value>,
    pub fcm_token: Option<String>,
    pub last_login_employee_id: Option<Uuid>,
    pub last_login_employee_name: Option<String>,
    pub last_login_employee_role: Option<Vec<String>>,
    pub app_version_code: Option<i32>,
    pub app_version_name: Option<String>,
    pub is_device_owner: bool,
    pub is_dev_device: bool,
    pub always_on: bool,
    pub watchdog_running: Option<bool>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub struct StagingTenkoSchedule {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub employee_id: Uuid,
    pub tenko_type: String,
    pub responsible_manager_name: String,
    pub scheduled_at: DateTime<Utc>,
    pub instruction: Option<String>,
    pub consumed: bool,
    pub consumed_by_session_id: Option<Uuid>,
    pub overdue_notified_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub struct StagingWebhookConfig {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub event_type: String,
    pub url: String,
    pub secret: Option<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub struct StagingTenantAllowedEmail {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub email: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub struct StagingSsoProviderConfig {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub provider: String,
    pub client_id: String,
    pub client_secret_encrypted: String,
    pub external_org_id: String,
    pub enabled: bool,
    pub woff_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub struct StagingTenkoCallNumber {
    pub id: i32,
    pub call_number: String,
    pub tenant_id: String,
    pub label: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub struct StagingTenkoCallDriver {
    pub id: i32,
    pub phone_number: String,
    pub driver_name: String,
    pub call_number: Option<String>,
    pub tenant_id: String,
    pub employee_code: Option<String>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Query params
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ExportParams {
    pub tenant_id: Uuid,
}

// ---------------------------------------------------------------------------
// Export handler
// ---------------------------------------------------------------------------

async fn export_handler(
    State(state): State<AppState>,
    Query(params): Query<ExportParams>,
) -> Result<Json<StagingExportData>, StatusCode> {
    if !is_staging_mode() {
        return Err(StatusCode::NOT_FOUND);
    }

    let pool = state.pool();
    let tid = params.tenant_id;
    let tid_str = tid.to_string();

    let tenant = sqlx::query_as::<_, StagingTenant>(
        "SELECT id, name, slug, email_domain, created_at FROM tenants WHERE id = $1",
    )
    .bind(tid)
    .fetch_optional(pool)
    .await
    .map_err(db_err)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let users = export_users(pool, tid).await?;
    let employees = export_employees(pool, tid).await?;
    let devices = export_devices(pool, tid).await?;
    let tenko_schedules = export_tenko_schedules(pool, tid).await?;
    let webhook_configs = export_webhook_configs(pool, tid).await?;
    let tenant_allowed_emails = export_allowed_emails(pool, tid).await?;
    let sso_provider_configs = export_sso_configs(pool, tid).await?;
    let tenko_call_numbers = export_tenko_call_numbers(pool, &tid_str).await?;
    let tenko_call_drivers = export_tenko_call_drivers(pool, &tid_str).await?;

    Ok(Json(StagingExportData {
        version: 1,
        exported_at: Utc::now().to_rfc3339(),
        tenant_id: tid_str,
        data: StagingData {
            tenant,
            users,
            employees,
            devices,
            tenko_schedules,
            webhook_configs,
            tenant_allowed_emails,
            sso_provider_configs,
            tenko_call_numbers,
            tenko_call_drivers,
        },
    }))
}

async fn export_users(pool: &PgPool, tid: Uuid) -> Result<Vec<StagingUser>, StatusCode> {
    sqlx::query_as::<_, StagingUser>(
        "SELECT id, tenant_id, google_sub, lineworks_id, email, name, role, created_at
         FROM users WHERE tenant_id = $1",
    )
    .bind(tid)
    .fetch_all(pool)
    .await
    .map_err(db_err)
}

async fn export_employees(pool: &PgPool, tid: Uuid) -> Result<Vec<StagingEmployee>, StatusCode> {
    sqlx::query_as::<_, StagingEmployee>(
        "SELECT id, tenant_id, code, nfc_id, name, face_photo_url, face_embedding,
                face_embedding_at, face_model_version, face_approval_status,
                face_approved_by, face_approved_at, license_issue_date, license_expiry_date,
                role, created_at, updated_at, deleted_at
         FROM employees WHERE tenant_id = $1",
    )
    .bind(tid)
    .fetch_all(pool)
    .await
    .map_err(db_err)
}

async fn export_devices(pool: &PgPool, tid: Uuid) -> Result<Vec<StagingDevice>, StatusCode> {
    sqlx::query_as::<_, StagingDevice>(
        "SELECT id, tenant_id, device_name, device_type, phone_number, user_id, status,
                approved_by, approved_at, last_seen_at, call_enabled, call_schedule,
                fcm_token, last_login_employee_id, last_login_employee_name,
                last_login_employee_role, app_version_code, app_version_name,
                is_device_owner, is_dev_device, always_on, watchdog_running,
                created_at, updated_at
         FROM devices WHERE tenant_id = $1",
    )
    .bind(tid)
    .fetch_all(pool)
    .await
    .map_err(db_err)
}

async fn export_tenko_schedules(
    pool: &PgPool,
    tid: Uuid,
) -> Result<Vec<StagingTenkoSchedule>, StatusCode> {
    sqlx::query_as::<_, StagingTenkoSchedule>(
        "SELECT id, tenant_id, employee_id, tenko_type, responsible_manager_name,
                scheduled_at, instruction, consumed, consumed_by_session_id,
                overdue_notified_at, created_at, updated_at
         FROM tenko_schedules WHERE tenant_id = $1",
    )
    .bind(tid)
    .fetch_all(pool)
    .await
    .map_err(db_err)
}

async fn export_webhook_configs(
    pool: &PgPool,
    tid: Uuid,
) -> Result<Vec<StagingWebhookConfig>, StatusCode> {
    sqlx::query_as::<_, StagingWebhookConfig>(
        "SELECT id, tenant_id, event_type, url, secret, enabled, created_at, updated_at
         FROM webhook_configs WHERE tenant_id = $1",
    )
    .bind(tid)
    .fetch_all(pool)
    .await
    .map_err(db_err)
}

async fn export_allowed_emails(
    pool: &PgPool,
    tid: Uuid,
) -> Result<Vec<StagingTenantAllowedEmail>, StatusCode> {
    sqlx::query_as::<_, StagingTenantAllowedEmail>(
        "SELECT id, tenant_id, email, role, created_at
         FROM tenant_allowed_emails WHERE tenant_id = $1",
    )
    .bind(tid)
    .fetch_all(pool)
    .await
    .map_err(db_err)
}

async fn export_sso_configs(
    pool: &PgPool,
    tid: Uuid,
) -> Result<Vec<StagingSsoProviderConfig>, StatusCode> {
    sqlx::query_as::<_, StagingSsoProviderConfig>(
        "SELECT id, tenant_id, provider, client_id, client_secret_encrypted,
                external_org_id, enabled, woff_id, created_at, updated_at
         FROM sso_provider_configs WHERE tenant_id = $1",
    )
    .bind(tid)
    .fetch_all(pool)
    .await
    .map_err(db_err)
}

async fn export_tenko_call_numbers(
    pool: &PgPool,
    tid: &str,
) -> Result<Vec<StagingTenkoCallNumber>, StatusCode> {
    sqlx::query_as::<_, StagingTenkoCallNumber>(
        "SELECT id, call_number, tenant_id, label, created_at
         FROM tenko_call_numbers WHERE tenant_id = $1",
    )
    .bind(tid)
    .fetch_all(pool)
    .await
    .map_err(db_err)
}

async fn export_tenko_call_drivers(
    pool: &PgPool,
    tid: &str,
) -> Result<Vec<StagingTenkoCallDriver>, StatusCode> {
    sqlx::query_as::<_, StagingTenkoCallDriver>(
        "SELECT id, phone_number, driver_name, call_number, tenant_id, employee_code, created_at
         FROM tenko_call_drivers WHERE tenant_id = $1",
    )
    .bind(tid)
    .fetch_all(pool)
    .await
    .map_err(db_err)
}

// ---------------------------------------------------------------------------
// Import handler
// ---------------------------------------------------------------------------

async fn import_handler(
    State(state): State<AppState>,
    Json(payload): Json<StagingExportData>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !is_staging_mode() {
        return Err(StatusCode::NOT_FOUND);
    }

    let pool = state.pool();
    let data = &payload.data;

    let mut tx = pool.begin().await.map_err(db_err)?;

    // 1. Tenant first (other tables reference it via FK)
    import_tenant(&mut tx, &data.tenant).await?;

    // 2. Set RLS tenant context for remaining inserts
    alc_core::tenant::set_current_tenant(&mut tx, &data.tenant.id.to_string())
        .await
        .map_err(db_err)?;

    // 3. Import all other tables
    let user_count = import_users(&mut tx, &data.users).await?;
    let employee_count = import_employees(&mut tx, &data.employees).await?;
    let device_count = import_devices(&mut tx, &data.devices).await?;
    let schedule_count = import_tenko_schedules(&mut tx, &data.tenko_schedules).await?;
    let webhook_count = import_webhook_configs(&mut tx, &data.webhook_configs).await?;
    let email_count = import_allowed_emails(&mut tx, &data.tenant_allowed_emails).await?;
    let sso_count = import_sso_configs(&mut tx, &data.sso_provider_configs).await?;
    let call_number_count = import_tenko_call_numbers(&mut tx, &data.tenko_call_numbers).await?;
    let call_driver_count = import_tenko_call_drivers(&mut tx, &data.tenko_call_drivers).await?;

    tx.commit().await.map_err(db_err)?;

    Ok(Json(json!({
        "status": "ok",
        "counts": {
            "tenants": 1,
            "users": user_count,
            "employees": employee_count,
            "devices": device_count,
            "tenko_schedules": schedule_count,
            "webhook_configs": webhook_count,
            "tenant_allowed_emails": email_count,
            "sso_provider_configs": sso_count,
            "tenko_call_numbers": call_number_count,
            "tenko_call_drivers": call_driver_count,
        }
    })))
}

type Tx<'a> = sqlx::Transaction<'a, sqlx::Postgres>;

async fn import_tenant(tx: &mut Tx<'_>, t: &StagingTenant) -> Result<(), StatusCode> {
    sqlx::query(
        "INSERT INTO tenants (id, name, slug, email_domain, created_at)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (id) DO UPDATE SET
           name = EXCLUDED.name,
           slug = EXCLUDED.slug,
           email_domain = EXCLUDED.email_domain",
    )
    .bind(t.id)
    .bind(&t.name)
    .bind(&t.slug)
    .bind(&t.email_domain)
    .bind(t.created_at)
    .execute(&mut **tx)
    .await
    .map_err(db_err)?;
    Ok(())
}

async fn import_users(tx: &mut Tx<'_>, users: &[StagingUser]) -> Result<usize, StatusCode> {
    for u in users {
        sqlx::query(
            "INSERT INTO users (id, tenant_id, google_sub, lineworks_id, email, name, role, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (id) DO UPDATE SET
               google_sub = EXCLUDED.google_sub,
               lineworks_id = EXCLUDED.lineworks_id,
               email = EXCLUDED.email,
               name = EXCLUDED.name,
               role = EXCLUDED.role",
        )
        .bind(u.id)
        .bind(u.tenant_id)
        .bind(&u.google_sub)
        .bind(&u.lineworks_id)
        .bind(&u.email)
        .bind(&u.name)
        .bind(&u.role)
        .bind(u.created_at)
        .execute(&mut **tx)
        .await
        .map_err(db_err)?;
    }
    Ok(users.len())
}

async fn import_employees(
    tx: &mut Tx<'_>,
    employees: &[StagingEmployee],
) -> Result<usize, StatusCode> {
    for e in employees {
        sqlx::query(
            "INSERT INTO employees (id, tenant_id, code, nfc_id, name, face_photo_url,
                face_embedding, face_embedding_at, face_model_version,
                face_approval_status, face_approved_by, face_approved_at,
                license_issue_date, license_expiry_date, role, created_at, updated_at, deleted_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
             ON CONFLICT (id) DO UPDATE SET
               code = EXCLUDED.code,
               nfc_id = EXCLUDED.nfc_id,
               name = EXCLUDED.name,
               face_photo_url = EXCLUDED.face_photo_url,
               face_embedding = EXCLUDED.face_embedding,
               face_embedding_at = EXCLUDED.face_embedding_at,
               face_model_version = EXCLUDED.face_model_version,
               face_approval_status = EXCLUDED.face_approval_status,
               face_approved_by = EXCLUDED.face_approved_by,
               face_approved_at = EXCLUDED.face_approved_at,
               license_issue_date = EXCLUDED.license_issue_date,
               license_expiry_date = EXCLUDED.license_expiry_date,
               role = EXCLUDED.role,
               updated_at = EXCLUDED.updated_at,
               deleted_at = EXCLUDED.deleted_at",
        )
        .bind(e.id)
        .bind(e.tenant_id)
        .bind(&e.code)
        .bind(&e.nfc_id)
        .bind(&e.name)
        .bind(&e.face_photo_url)
        .bind(&e.face_embedding)
        .bind(e.face_embedding_at)
        .bind(&e.face_model_version)
        .bind(&e.face_approval_status)
        .bind(e.face_approved_by)
        .bind(e.face_approved_at)
        .bind(e.license_issue_date)
        .bind(e.license_expiry_date)
        .bind(&e.role)
        .bind(e.created_at)
        .bind(e.updated_at)
        .bind(e.deleted_at)
        .execute(&mut **tx)
        .await
        .map_err(db_err)?;
    }
    Ok(employees.len())
}

async fn import_devices(tx: &mut Tx<'_>, devices: &[StagingDevice]) -> Result<usize, StatusCode> {
    for d in devices {
        sqlx::query(
            "INSERT INTO devices (id, tenant_id, device_name, device_type, phone_number,
                user_id, status, approved_by, approved_at, last_seen_at, call_enabled,
                call_schedule, fcm_token, last_login_employee_id, last_login_employee_name,
                last_login_employee_role, app_version_code, app_version_name,
                is_device_owner, is_dev_device, always_on, watchdog_running,
                created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24)
             ON CONFLICT (id) DO UPDATE SET
               device_name = EXCLUDED.device_name,
               device_type = EXCLUDED.device_type,
               phone_number = EXCLUDED.phone_number,
               user_id = EXCLUDED.user_id,
               status = EXCLUDED.status,
               call_enabled = EXCLUDED.call_enabled,
               call_schedule = EXCLUDED.call_schedule,
               fcm_token = EXCLUDED.fcm_token,
               is_device_owner = EXCLUDED.is_device_owner,
               is_dev_device = EXCLUDED.is_dev_device,
               always_on = EXCLUDED.always_on,
               watchdog_running = EXCLUDED.watchdog_running,
               updated_at = EXCLUDED.updated_at",
        )
        .bind(d.id)
        .bind(d.tenant_id)
        .bind(&d.device_name)
        .bind(&d.device_type)
        .bind(&d.phone_number)
        .bind(d.user_id)
        .bind(&d.status)
        .bind(d.approved_by)
        .bind(d.approved_at)
        .bind(d.last_seen_at)
        .bind(d.call_enabled)
        .bind(&d.call_schedule)
        .bind(&d.fcm_token)
        .bind(d.last_login_employee_id)
        .bind(&d.last_login_employee_name)
        .bind(&d.last_login_employee_role)
        .bind(d.app_version_code)
        .bind(&d.app_version_name)
        .bind(d.is_device_owner)
        .bind(d.is_dev_device)
        .bind(d.always_on)
        .bind(d.watchdog_running)
        .bind(d.created_at)
        .bind(d.updated_at)
        .execute(&mut **tx)
        .await
        .map_err(db_err)?;
    }
    Ok(devices.len())
}

async fn import_tenko_schedules(
    tx: &mut Tx<'_>,
    schedules: &[StagingTenkoSchedule],
) -> Result<usize, StatusCode> {
    for s in schedules {
        sqlx::query(
            "INSERT INTO tenko_schedules (id, tenant_id, employee_id, tenko_type,
                responsible_manager_name, scheduled_at, instruction, consumed,
                consumed_by_session_id, overdue_notified_at, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
             ON CONFLICT (id) DO UPDATE SET
               tenko_type = EXCLUDED.tenko_type,
               responsible_manager_name = EXCLUDED.responsible_manager_name,
               scheduled_at = EXCLUDED.scheduled_at,
               instruction = EXCLUDED.instruction,
               consumed = EXCLUDED.consumed,
               consumed_by_session_id = EXCLUDED.consumed_by_session_id,
               updated_at = EXCLUDED.updated_at",
        )
        .bind(s.id)
        .bind(s.tenant_id)
        .bind(s.employee_id)
        .bind(&s.tenko_type)
        .bind(&s.responsible_manager_name)
        .bind(s.scheduled_at)
        .bind(&s.instruction)
        .bind(s.consumed)
        .bind(s.consumed_by_session_id)
        .bind(s.overdue_notified_at)
        .bind(s.created_at)
        .bind(s.updated_at)
        .execute(&mut **tx)
        .await
        .map_err(db_err)?;
    }
    Ok(schedules.len())
}

async fn import_webhook_configs(
    tx: &mut Tx<'_>,
    configs: &[StagingWebhookConfig],
) -> Result<usize, StatusCode> {
    for c in configs {
        sqlx::query(
            "INSERT INTO webhook_configs (id, tenant_id, event_type, url, secret, enabled, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (id) DO UPDATE SET
               event_type = EXCLUDED.event_type,
               url = EXCLUDED.url,
               secret = EXCLUDED.secret,
               enabled = EXCLUDED.enabled,
               updated_at = EXCLUDED.updated_at",
        )
        .bind(c.id)
        .bind(c.tenant_id)
        .bind(&c.event_type)
        .bind(&c.url)
        .bind(&c.secret)
        .bind(c.enabled)
        .bind(c.created_at)
        .bind(c.updated_at)
        .execute(&mut **tx)
        .await
        .map_err(db_err)?;
    }
    Ok(configs.len())
}

async fn import_allowed_emails(
    tx: &mut Tx<'_>,
    emails: &[StagingTenantAllowedEmail],
) -> Result<usize, StatusCode> {
    for e in emails {
        sqlx::query(
            "INSERT INTO tenant_allowed_emails (id, tenant_id, email, role, created_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (id) DO UPDATE SET
               email = EXCLUDED.email,
               role = EXCLUDED.role",
        )
        .bind(e.id)
        .bind(e.tenant_id)
        .bind(&e.email)
        .bind(&e.role)
        .bind(e.created_at)
        .execute(&mut **tx)
        .await
        .map_err(db_err)?;
    }
    Ok(emails.len())
}

async fn import_sso_configs(
    tx: &mut Tx<'_>,
    configs: &[StagingSsoProviderConfig],
) -> Result<usize, StatusCode> {
    for c in configs {
        sqlx::query(
            "INSERT INTO sso_provider_configs (id, tenant_id, provider, client_id,
                client_secret_encrypted, external_org_id, enabled, woff_id, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT (id) DO UPDATE SET
               client_id = EXCLUDED.client_id,
               client_secret_encrypted = EXCLUDED.client_secret_encrypted,
               external_org_id = EXCLUDED.external_org_id,
               enabled = EXCLUDED.enabled,
               woff_id = EXCLUDED.woff_id,
               updated_at = EXCLUDED.updated_at",
        )
        .bind(c.id)
        .bind(c.tenant_id)
        .bind(&c.provider)
        .bind(&c.client_id)
        .bind(&c.client_secret_encrypted)
        .bind(&c.external_org_id)
        .bind(c.enabled)
        .bind(&c.woff_id)
        .bind(c.created_at)
        .bind(c.updated_at)
        .execute(&mut **tx)
        .await
        .map_err(db_err)?;
    }
    Ok(configs.len())
}

async fn import_tenko_call_numbers(
    tx: &mut Tx<'_>,
    numbers: &[StagingTenkoCallNumber],
) -> Result<usize, StatusCode> {
    for n in numbers {
        sqlx::query(
            "INSERT INTO tenko_call_numbers (id, call_number, tenant_id, label, created_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (id) DO UPDATE SET
               call_number = EXCLUDED.call_number,
               label = EXCLUDED.label",
        )
        .bind(n.id)
        .bind(&n.call_number)
        .bind(&n.tenant_id)
        .bind(&n.label)
        .bind(n.created_at)
        .execute(&mut **tx)
        .await
        .map_err(db_err)?;
    }
    Ok(numbers.len())
}

async fn import_tenko_call_drivers(
    tx: &mut Tx<'_>,
    drivers: &[StagingTenkoCallDriver],
) -> Result<usize, StatusCode> {
    for d in drivers {
        sqlx::query(
            "INSERT INTO tenko_call_drivers (id, phone_number, driver_name, call_number,
                tenant_id, employee_code, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (id) DO UPDATE SET
               phone_number = EXCLUDED.phone_number,
               driver_name = EXCLUDED.driver_name,
               call_number = EXCLUDED.call_number,
               employee_code = EXCLUDED.employee_code",
        )
        .bind(d.id)
        .bind(&d.phone_number)
        .bind(&d.driver_name)
        .bind(&d.call_number)
        .bind(&d.tenant_id)
        .bind(&d.employee_code)
        .bind(d.created_at)
        .execute(&mut **tx)
        .await
        .map_err(db_err)?;
    }
    Ok(drivers.len())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn db_err(e: sqlx::Error) -> StatusCode {
    tracing::error!("staging db error: {e}");
    StatusCode::INTERNAL_SERVER_ERROR
}

use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::constant_time::constant_time_eq;
use alc_core::AppState;

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router() -> Router<AppState> {
    assert_not_prod_with_staging_mode();
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

/// 本番 fail-safe (Refs #391)。Cloud Run の `K_SERVICE` (service 名) が
/// "staging" を含まない service で `STAGING_MODE=true` が立っていたら、無認証
/// export/import が本番データに開くため router 構築時点で起動を拒否する。
/// ローカル dev / テストは `K_SERVICE` 自体が無いので影響しない。
fn assert_not_prod_with_staging_mode() {
    if let Ok(service) = std::env::var("K_SERVICE") {
        if !service.contains("staging") && is_staging_mode() {
            panic!(
                "STAGING_MODE=true must not be set on non-staging service '{service}' — refusing to boot (Refs #391)"
            );
        }
    }
}

/// shared-secret ヘッダ検証 (Refs #391)。`STAGING_API_KEY` が設定されていれば
/// `X-Staging-Key` ヘッダとの constant-time 一致を要求し、同一 staging 内の
/// 任意 tenant_id dump / 改竄を「key を知る caller」に限定する。
/// 未設定なら従来どおり無認証 (揮発 DB 前提の staging-open 運用を壊さない
/// opt-in 強化。env + フロント側ヘッダ送信が揃った時点で有効化する)。
fn check_staging_key(headers: &HeaderMap) -> Result<(), StatusCode> {
    let Ok(expected) = std::env::var("STAGING_API_KEY") else {
        return Ok(());
    };
    let provided = headers
        .get("X-Staging-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
        Ok(())
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

// ---------------------------------------------------------------------------
// Macros — export / import boilerplate elimination
// ---------------------------------------------------------------------------

/// Generate an export function: `SELECT <cols> FROM <table> WHERE tenant_id = $1`
macro_rules! staging_export {
    ($fn_name:ident, $struct_ty:ty, $table:expr, $tid_ty:ty, [$($col:ident),+ $(,)?]) => {
        async fn $fn_name(pool: &PgPool, tid: $tid_ty) -> Result<Vec<$struct_ty>, StatusCode> {
            sqlx::query_as::<_, $struct_ty>(
                concat!(
                    "SELECT ",
                    staging_export!(@cols $($col),+),
                    " FROM ", $table, " WHERE tenant_id = $1"
                )
            )
            .bind(tid)
            .fetch_all(pool)
            .await
            .map_err(db_err)
        }
    };
    (@cols $col:ident) => { stringify!($col) };
    (@cols $col:ident, $($rest:ident),+) => {
        concat!(stringify!($col), ", ", staging_export!(@cols $($rest),+))
    };
}

/// Build an UPSERT SQL string at runtime (avoids compile-time counter limitation).
fn build_upsert_sql(table: &str, insert_cols: &[&str], update_cols: &[&str]) -> String {
    let placeholders = (1..=insert_cols.len())
        .map(|i| format!("${i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let cols = insert_cols.join(", ");
    let updates = update_cols
        .iter()
        .map(|c| format!("{c} = EXCLUDED.{c}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("INSERT INTO {table} ({cols}) VALUES ({placeholders}) ON CONFLICT (id) DO UPDATE SET {updates}")
}

/// Generate an import function: loop + UPSERT with `.bind(&item.field)` chain.
macro_rules! staging_import {
    (
        $fn_name:ident, $struct_ty:ty, $table:expr,
        insert: [$($icol:ident),+ $(,)?],
        update: [$($ucol:ident),+ $(,)?]
    ) => {
        async fn $fn_name(tx: &mut Tx<'_>, items: &[$struct_ty]) -> Result<usize, StatusCode> {
            let sql = build_upsert_sql(
                $table,
                &[$(stringify!($icol)),+],
                &[$(stringify!($ucol)),+],
            );
            for item in items {
                sqlx::query(&sql)
                    $( .bind(&item.$icol) )+
                    .execute(&mut **tx)
                    .await
                    .map_err(db_err)?;
            }
            Ok(items.len())
        }
    };
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
    #[serde(default)]
    pub bot_configs: Vec<StagingBotConfig>,
    #[serde(default)]
    pub notify_line_configs: Vec<StagingNotifyLineConfig>,
    #[serde(default)]
    pub notify_recipients: Vec<StagingNotifyRecipient>,
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
    pub line_user_id: Option<String>,
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

#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub struct StagingBotConfig {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub provider: String,
    pub name: String,
    pub client_id: String,
    pub client_secret_encrypted: String,
    pub service_account: String,
    pub private_key_encrypted: String,
    pub bot_id: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub struct StagingNotifyLineConfig {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub channel_id: String,
    pub channel_secret_encrypted: String,
    pub channel_access_token_encrypted: Option<String>,
    pub key_id: Option<String>,
    pub private_key_encrypted: Option<String>,
    pub bot_basic_id: Option<String>,
    pub login_channel_id: Option<String>,
    pub login_channel_secret_encrypted: Option<String>,
    pub public_key_jwk: Option<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub struct StagingNotifyRecipient {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub provider: String,
    pub lineworks_user_id: Option<String>,
    pub line_user_id: Option<String>,
    pub phone_number: Option<String>,
    pub email: Option<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Query params
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ExportParams {
    pub tenant_id: Uuid,
}

// ---------------------------------------------------------------------------
// Export functions (macro-generated)
// ---------------------------------------------------------------------------

staging_export!(
    export_users,
    StagingUser,
    "users",
    Uuid,
    [
        id,
        tenant_id,
        google_sub,
        lineworks_id,
        line_user_id,
        email,
        name,
        role,
        created_at
    ]
);

staging_export!(
    export_employees,
    StagingEmployee,
    "employees",
    Uuid,
    [
        id,
        tenant_id,
        code,
        nfc_id,
        name,
        face_photo_url,
        face_embedding,
        face_embedding_at,
        face_model_version,
        face_approval_status,
        face_approved_by,
        face_approved_at,
        license_issue_date,
        license_expiry_date,
        role,
        created_at,
        updated_at,
        deleted_at
    ]
);

staging_export!(
    export_devices,
    StagingDevice,
    "devices",
    Uuid,
    [
        id,
        tenant_id,
        device_name,
        device_type,
        phone_number,
        user_id,
        status,
        approved_by,
        approved_at,
        last_seen_at,
        call_enabled,
        call_schedule,
        fcm_token,
        last_login_employee_id,
        last_login_employee_name,
        last_login_employee_role,
        app_version_code,
        app_version_name,
        is_device_owner,
        is_dev_device,
        always_on,
        watchdog_running,
        created_at,
        updated_at
    ]
);

staging_export!(
    export_tenko_schedules,
    StagingTenkoSchedule,
    "tenko_schedules",
    Uuid,
    [
        id,
        tenant_id,
        employee_id,
        tenko_type,
        responsible_manager_name,
        scheduled_at,
        instruction,
        consumed,
        consumed_by_session_id,
        overdue_notified_at,
        created_at,
        updated_at
    ]
);

staging_export!(
    export_webhook_configs,
    StagingWebhookConfig,
    "webhook_configs",
    Uuid,
    [id, tenant_id, event_type, url, secret, enabled, created_at, updated_at]
);

staging_export!(
    export_allowed_emails,
    StagingTenantAllowedEmail,
    "tenant_allowed_emails",
    Uuid,
    [id, tenant_id, email, role, created_at]
);

staging_export!(
    export_sso_configs,
    StagingSsoProviderConfig,
    "sso_provider_configs",
    Uuid,
    [
        id,
        tenant_id,
        provider,
        client_id,
        client_secret_encrypted,
        external_org_id,
        enabled,
        woff_id,
        created_at,
        updated_at
    ]
);

staging_export!(
    export_tenko_call_numbers,
    StagingTenkoCallNumber,
    "tenko_call_numbers",
    &str,
    [id, call_number, tenant_id, label, created_at]
);

staging_export!(
    export_tenko_call_drivers,
    StagingTenkoCallDriver,
    "tenko_call_drivers",
    &str,
    [
        id,
        phone_number,
        driver_name,
        call_number,
        tenant_id,
        employee_code,
        created_at
    ]
);

staging_export!(
    export_bot_configs,
    StagingBotConfig,
    "bot_configs",
    Uuid,
    [
        id,
        tenant_id,
        provider,
        name,
        client_id,
        client_secret_encrypted,
        service_account,
        private_key_encrypted,
        bot_id,
        enabled,
        created_at,
        updated_at
    ]
);

staging_export!(
    export_notify_line_configs,
    StagingNotifyLineConfig,
    "notify_line_configs",
    Uuid,
    [
        id,
        tenant_id,
        name,
        channel_id,
        channel_secret_encrypted,
        channel_access_token_encrypted,
        key_id,
        private_key_encrypted,
        bot_basic_id,
        login_channel_id,
        login_channel_secret_encrypted,
        public_key_jwk,
        enabled,
        created_at,
        updated_at
    ]
);

staging_export!(
    export_notify_recipients,
    StagingNotifyRecipient,
    "notify_recipients",
    Uuid,
    [
        id,
        tenant_id,
        name,
        provider,
        lineworks_user_id,
        line_user_id,
        phone_number,
        email,
        enabled,
        created_at,
        updated_at
    ]
);

// ---------------------------------------------------------------------------
// Export handler
// ---------------------------------------------------------------------------

async fn export_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<ExportParams>,
) -> Result<Json<StagingExportData>, StatusCode> {
    if !is_staging_mode() {
        return Err(StatusCode::NOT_FOUND);
    }
    check_staging_key(&headers)?;

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
    // webhook の平文 secret は export に載せない (Refs #391)。import 側も secret
    // カラムを触らないため、復元時は既存値保持 / 新規行は NULL になる
    let mut webhook_configs = export_webhook_configs(pool, tid).await?;
    for c in &mut webhook_configs {
        c.secret = None;
    }
    let tenant_allowed_emails = export_allowed_emails(pool, tid).await?;
    let sso_provider_configs = export_sso_configs(pool, tid).await?;
    let tenko_call_numbers = export_tenko_call_numbers(pool, &tid_str).await?;
    let tenko_call_drivers = export_tenko_call_drivers(pool, &tid_str).await?;
    let bot_configs = export_bot_configs(pool, tid).await?;
    let notify_line_configs = export_notify_line_configs(pool, tid).await?;
    let notify_recipients = export_notify_recipients(pool, tid).await?;

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
            bot_configs,
            notify_line_configs,
            notify_recipients,
        },
    }))
}

// ---------------------------------------------------------------------------
// Import functions (macro-generated)
// ---------------------------------------------------------------------------

type Tx<'a> = sqlx::Transaction<'a, sqlx::Postgres>;

staging_import!(import_tenants, StagingTenant, "tenants",
    insert: [id, name, slug, email_domain, created_at],
    update: [name, slug, email_domain]);

staging_import!(import_users, StagingUser, "users",
    insert: [id, tenant_id, google_sub, lineworks_id, line_user_id, email, name, role, created_at],
    update: [google_sub, lineworks_id, line_user_id, email, name, role]);

staging_import!(import_employees, StagingEmployee, "employees",
    insert: [id, tenant_id, code, nfc_id, name, face_photo_url,
             face_embedding, face_embedding_at, face_model_version,
             face_approval_status, face_approved_by, face_approved_at,
             license_issue_date, license_expiry_date, role, created_at, updated_at, deleted_at],
    update: [code, nfc_id, name, face_photo_url, face_embedding, face_embedding_at,
             face_model_version, face_approval_status, face_approved_by, face_approved_at,
             license_issue_date, license_expiry_date, role, updated_at, deleted_at]);

staging_import!(import_devices, StagingDevice, "devices",
    insert: [id, tenant_id, device_name, device_type, phone_number,
             user_id, status, approved_by, approved_at, last_seen_at, call_enabled,
             call_schedule, fcm_token, last_login_employee_id, last_login_employee_name,
             last_login_employee_role, app_version_code, app_version_name,
             is_device_owner, is_dev_device, always_on, watchdog_running,
             created_at, updated_at],
    update: [device_name, device_type, phone_number, user_id, status,
             call_enabled, call_schedule, fcm_token,
             is_device_owner, is_dev_device, always_on, watchdog_running, updated_at]);

staging_import!(import_tenko_schedules, StagingTenkoSchedule, "tenko_schedules",
    insert: [id, tenant_id, employee_id, tenko_type,
             responsible_manager_name, scheduled_at, instruction, consumed,
             consumed_by_session_id, overdue_notified_at, created_at, updated_at],
    update: [tenko_type, responsible_manager_name, scheduled_at, instruction,
             consumed, consumed_by_session_id, updated_at]);

// secret は insert/update とも対象外 (Refs #391): export が secret を含まなくなった
// ため、import で既存 secret を NULL 上書きしない。新規行は NULL (要再設定)。
staging_import!(import_webhook_configs, StagingWebhookConfig, "webhook_configs",
    insert: [id, tenant_id, event_type, url, enabled, created_at, updated_at],
    update: [event_type, url, enabled, updated_at]);

staging_import!(import_allowed_emails, StagingTenantAllowedEmail, "tenant_allowed_emails",
    insert: [id, tenant_id, email, role, created_at],
    update: [email, role]);

staging_import!(import_sso_configs, StagingSsoProviderConfig, "sso_provider_configs",
    insert: [id, tenant_id, provider, client_id,
             client_secret_encrypted, external_org_id, enabled, woff_id, created_at, updated_at],
    update: [client_id, client_secret_encrypted, external_org_id, enabled, woff_id, updated_at]);

staging_import!(import_tenko_call_numbers, StagingTenkoCallNumber, "tenko_call_numbers",
    insert: [id, call_number, tenant_id, label, created_at],
    update: [call_number, label]);

staging_import!(import_tenko_call_drivers, StagingTenkoCallDriver, "tenko_call_drivers",
    insert: [id, phone_number, driver_name, call_number, tenant_id, employee_code, created_at],
    update: [phone_number, driver_name, call_number, employee_code]);

staging_import!(import_bot_configs, StagingBotConfig, "bot_configs",
    insert: [id, tenant_id, provider, name, client_id, client_secret_encrypted,
             service_account, private_key_encrypted, bot_id, enabled, created_at, updated_at],
    update: [provider, name, client_id, client_secret_encrypted,
             service_account, private_key_encrypted, bot_id, enabled, updated_at]);

staging_import!(import_notify_line_configs, StagingNotifyLineConfig, "notify_line_configs",
    insert: [id, tenant_id, name, channel_id, channel_secret_encrypted,
             channel_access_token_encrypted, key_id, private_key_encrypted, bot_basic_id,
             login_channel_id, login_channel_secret_encrypted, public_key_jwk,
             enabled, created_at, updated_at],
    update: [name, channel_id, channel_secret_encrypted, channel_access_token_encrypted,
             key_id, private_key_encrypted, bot_basic_id, login_channel_id,
             login_channel_secret_encrypted, public_key_jwk, enabled, updated_at]);

staging_import!(import_notify_recipients, StagingNotifyRecipient, "notify_recipients",
    insert: [id, tenant_id, name, provider, lineworks_user_id, line_user_id,
             phone_number, email, enabled, created_at, updated_at],
    update: [name, provider, lineworks_user_id, line_user_id,
             phone_number, email, enabled, updated_at]);

// ---------------------------------------------------------------------------
// Import handler
// ---------------------------------------------------------------------------

async fn import_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !is_staging_mode() {
        return Err(StatusCode::NOT_FOUND);
    }
    // key 検証を本文 parse より前に行うため Json extractor ではなく Bytes で受ける。
    // Json だと不正本文が parse 段階 (422) で弾かれ key チェックに到達しないため、
    // export (Query) と挙動を揃えて「誤 key は本文の正否によらず 401」にする。Refs #391
    check_staging_key(&headers)?;

    let payload: StagingExportData = serde_json::from_slice(&body).map_err(|e| {
        tracing::warn!("staging import parse error: {e}");
        StatusCode::BAD_REQUEST
    })?;

    let pool = state.pool();
    let data = &payload.data;

    let mut tx = pool.begin().await.map_err(db_err)?;

    // 1. Tenant first (other tables reference it via FK)
    import_tenants(&mut tx, std::slice::from_ref(&data.tenant)).await?;

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
    let bot_config_count = import_bot_configs(&mut tx, &data.bot_configs).await?;
    let notify_line_config_count =
        import_notify_line_configs(&mut tx, &data.notify_line_configs).await?;
    let notify_recipient_count = import_notify_recipients(&mut tx, &data.notify_recipients).await?;

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
            "bot_configs": bot_config_count,
            "notify_line_configs": notify_line_config_count,
            "notify_recipients": notify_recipient_count,
        }
    })))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn db_err(e: sqlx::Error) -> StatusCode {
    tracing::error!("staging db error: {e}");
    StatusCode::INTERNAL_SERVER_ERROR
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// env (K_SERVICE / STAGING_MODE / STAGING_API_KEY) を触るテストの直列化
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn clear_env() {
        std::env::remove_var("K_SERVICE");
        std::env::remove_var("STAGING_MODE");
        std::env::remove_var("STAGING_API_KEY");
    }

    #[test]
    fn prod_guard_panics_when_staging_mode_on_prod_service() {
        let guard = ENV_LOCK.lock().unwrap();
        clear_env();
        std::env::set_var("K_SERVICE", "rust-alc-api");
        std::env::set_var("STAGING_MODE", "true");
        let result = std::panic::catch_unwind(assert_not_prod_with_staging_mode);
        clear_env();
        drop(guard);
        assert!(
            result.is_err(),
            "expected panic on prod + STAGING_MODE=true"
        );
    }

    #[test]
    fn prod_guard_allows_staging_service_and_prod_without_flag() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_env();
        // staging service なら STAGING_MODE=true でも boot できる
        std::env::set_var("K_SERVICE", "rust-alc-api-staging");
        std::env::set_var("STAGING_MODE", "true");
        assert_not_prod_with_staging_mode();
        // 本番 service でも STAGING_MODE が無ければ boot できる
        std::env::set_var("K_SERVICE", "rust-alc-api");
        std::env::remove_var("STAGING_MODE");
        assert_not_prod_with_staging_mode();
        // ローカル (K_SERVICE 無し) は STAGING_MODE=true でも boot できる
        clear_env();
        std::env::set_var("STAGING_MODE", "true");
        assert_not_prod_with_staging_mode();
        clear_env();
    }

    #[test]
    fn staging_key_optional_when_env_unset() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_env();
        let headers = HeaderMap::new();
        assert!(check_staging_key(&headers).is_ok());
    }

    #[test]
    fn staging_key_enforced_when_env_set() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_env();
        std::env::set_var("STAGING_API_KEY", "test-staging-key");

        // ヘッダ無し → 401
        let headers = HeaderMap::new();
        assert_eq!(check_staging_key(&headers), Err(StatusCode::UNAUTHORIZED));

        // 不一致 → 401
        let mut headers = HeaderMap::new();
        headers.insert("X-Staging-Key", "wrong".parse().unwrap());
        assert_eq!(check_staging_key(&headers), Err(StatusCode::UNAUTHORIZED));

        // 一致 → Ok
        let mut headers = HeaderMap::new();
        headers.insert("X-Staging-Key", "test-staging-key".parse().unwrap());
        assert!(check_staging_key(&headers).is_ok());

        clear_env();
    }
}

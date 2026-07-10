use std::sync::Arc;

use axum::http::header::{
    HeaderValue, REFERRER_POLICY, STRICT_TRANSPORT_SECURITY, X_CONTENT_TYPE_OPTIONS,
    X_FRAME_OPTIONS,
};
use axum::http::Method;
use axum::{Extension, Router};
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use sqlx::types::Uuid;

use alc_camera::repo::PgCamerasRepository;
use alc_camera::{CameraDownTicket, CameraState, DownTicketSink, DEFAULT_DOWN_THRESHOLD};
use alc_notify::repo::{
    PgLineworksChannelsRepository, PgNotifyDeliveryRepository, PgNotifyDocumentRepository,
    PgNotifyGroupRepository, PgNotifyLineConfigRepository, PgNotifyRecipientRepository,
};
use alc_trouble::models::CreateTroubleTicket;
use alc_trouble::repo::{
    trouble_categories::PgTroubleCategoriesRepository,
    trouble_field_layouts::PgTroubleFieldLayoutsRepository,
    trouble_files::PgTroubleFilesRepository,
    trouble_notification_prefs::PgTroubleNotificationPrefsRepository,
    trouble_offices::PgTroubleOfficesRepository,
    trouble_progress_statuses::PgTroubleProgressStatusesRepository,
    trouble_schedules::PgTroubleSchedulesRepository,
    trouble_task_statuses::PgTroubleTaskStatusesRepository,
    trouble_task_types::PgTroubleTaskTypesRepository, trouble_tasks::PgTroubleTasksRepository,
    trouble_tickets::PgTroubleTicketsRepository, trouble_workflow::PgTroubleWorkflowRepository,
};
use alc_trouble::repository::TroubleTicketsRepository;
use rust_alc_api::db::repository::{
    PgApiTokensRepository, PgAuthRepository, PgBotAdminRepository, PgCarInspectionRepository,
    PgCarinsFilesRepository, PgCarryingItemsRepository, PgCommunicationItemsRepository,
    PgDailyHealthRepository, PgDeviceRepository, PgDriverInfoRepository, PgDtakoCsvProxyRepository,
    PgDtakoDailyHoursRepository, PgDtakoDriversRepository, PgDtakoEventClassificationsRepository,
    PgDtakoLogsRepository, PgDtakoOperationsRepository, PgDtakoRestraintReportPdfRepository,
    PgDtakoRestraintReportRepository, PgDtakoScraperRepository, PgDtakoTicketsRepository,
    PgDtakoUploadRepository, PgDtakoVehiclesRepository, PgDtakoWorkTimesRepository,
    PgDtakoYTimeExportRepository, PgEmployeeRepository, PgEquipmentFailuresRepository,
    PgGuidanceRecordsRepository, PgHealthBaselinesRepository, PgItemFilesRepository,
    PgItemsRepository, PgMeasurementsRepository, PgNfcTagRepository, PgSsoAdminRepository,
    PgTenantUsersRepository, PgTenkoCallRepository, PgTenkoRecordsRepository,
    PgTenkoSchedulesRepository, PgTenkoSessionRepository, PgTenkoWebhooksRepository,
    PgTimecardRepository, PgVehicleSettingsDumpsRepository,
};
use rust_alc_api::storage::StorageBackend;
use rust_alc_api::AppState;

/// camera 所有 port [`DownTicketSink`] → alc-trouble への adapter (Refs #556、旧
/// `crates/alc-camera-api/src/main.rs` から移設)。category / custom_fields マーカー
/// 等の trouble 語彙への写像はここが持ち、alc-camera crate は trouble に依存しない。
struct TroubleDownTicketSink(Arc<dyn TroubleTicketsRepository>);

#[async_trait::async_trait]
impl DownTicketSink for TroubleDownTicketSink {
    async fn open_down_ticket(
        &self,
        tenant_id: Uuid,
        t: CameraDownTicket,
    ) -> Result<Uuid, sqlx::Error> {
        let input = CreateTroubleTicket {
            category: "その他".to_string(),
            title: Some(t.title),
            occurred_at: Some(t.occurred_at),
            location: Some(t.location),
            description: Some(t.description),
            // 自動起票である事を識別するマーカー (UI の出所表示 / 集計用)。
            custom_fields: Some(serde_json::json!({
                "source": "camera_auto",
                "camera_id": t.camera_id,
            })),
            ..Default::default()
        };
        let ticket = self.0.create(tenant_id, &input, None, None).await?;
        Ok(ticket.id)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // JSON formatter: Cloud Run Logging が severity (info/warn/error) を解釈でき、
    // jsonPayload.<field> で構造化ログ (redact_pipeline_done の llm_ms 等) を
    // クエリできるようにする (Refs #333)。text formatter だと severity が常に
    // DEFAULT 扱いになり `severity>=ERROR` が 0 件になる。
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    dotenvy::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".into())
        .parse()
        .expect("PORT must be a number");

    // NOTE: 旧ログイン用 Google verifier (GOOGLE_CLIENT_ID / GOOGLE_DEVICE_CLIENT_ID) は
    // 撤去 — ログインは auth-worker に完全移管済み (#479 PR-3) で、この verifier を
    // Extension で取り出す handler は存在しなかった (dead layer)。rust 側に残る
    // Google JWKS 検証は internal OIDC (aud=alc-api-internal、routes::router 内で構築)
    // のみ。

    let pool = PgPoolOptions::new()
        .max_connections(10)
        // RLS テナント GUC (`app.current_tenant_id`) は `set_current_tenant` が
        // session-scope (`set_config(..., false)`) で立てるため、コネクションを
        // プールへ返してもクリアされない。次に同じコネクションを borrow した
        // 別リクエスト (特に `self.pool` 直クエリ) へ tenant コンテキストが
        // リークするのを防ぐため、返却時に必ず RESET して未設定 (= RLS で
        // 行ゼロの fail-closed) に戻す。Refs #386。
        .after_release(|conn, _meta| {
            Box::pin(async move {
                sqlx::query("RESET app.current_tenant_id")
                    .execute(conn)
                    .await?;
                Ok(true)
            })
        })
        .connect(&database_url)
        .await?;

    // Storage backend selection
    let storage_backend = std::env::var("STORAGE_BACKEND").unwrap_or_else(|_| "gcs".into());
    let storage: Arc<dyn StorageBackend> = match storage_backend.as_str() {
        "r2" => {
            let bucket =
                std::env::var("R2_BUCKET").expect("R2_BUCKET required when STORAGE_BACKEND=r2");
            let account_id = std::env::var("R2_ACCOUNT_ID")
                .expect("R2_ACCOUNT_ID required when STORAGE_BACKEND=r2");
            let access_key = std::env::var("R2_ACCESS_KEY")
                .expect("R2_ACCESS_KEY required when STORAGE_BACKEND=r2");
            let secret_key = std::env::var("R2_SECRET_KEY")
                .expect("R2_SECRET_KEY required when STORAGE_BACKEND=r2");
            let public_url = std::env::var("R2_PUBLIC_URL_BASE").ok();

            tracing::info!("Storage backend: R2 (bucket={})", bucket);
            Arc::new(
                rust_alc_api::storage::R2Backend::new(
                    bucket, account_id, access_key, secret_key, public_url,
                )
                .expect("Failed to initialize R2 backend"),
            )
        }
        _ => {
            let bucket = std::env::var("GCS_BUCKET").unwrap_or_else(|_| "alc-face-photos".into());
            tracing::info!("Storage backend: GCS (bucket={})", bucket);
            Arc::new(rust_alc_api::storage::GcsBackend::new(bucket))
        }
    };

    // carins ファイル用 R2 (carins-files バケット、別 API トークン)
    let carins_storage: Option<Arc<dyn StorageBackend>> =
        std::env::var("CARINS_R2_BUCKET").ok().map(|bucket| {
            let account_id = std::env::var("R2_ACCOUNT_ID")
                .expect("R2_ACCOUNT_ID required for CARINS_R2_BUCKET");
            let access_key =
                std::env::var("CARINS_R2_ACCESS_KEY").expect("CARINS_R2_ACCESS_KEY required");
            let secret_key =
                std::env::var("CARINS_R2_SECRET_KEY").expect("CARINS_R2_SECRET_KEY required");
            tracing::info!("Carins storage: R2 (bucket={})", bucket);
            Arc::new(
                rust_alc_api::storage::R2Backend::new(
                    bucket, account_id, access_key, secret_key, None,
                )
                .expect("Failed to init carins R2 backend"),
            ) as Arc<dyn StorageBackend>
        });

    // dtako (digitacho) 用 R2 (ohishi-dtako バケット、別 API トークン)。
    //
    // dev サンドボックス (Incus) では `DTAKO_STORAGE_HTTP_PROXY` を設定すると、ホスト側の
    // `wrangler dev --local --port 8788` (R2 binding 付き) 経由で R2 にアクセスする。
    // この場合 Incus に R2 keys を投入する必要が無い (本番 R2 は `R2_ACCESS_KEY` 等 経由のみ)。
    let dtako_storage: Option<Arc<dyn StorageBackend>> =
        if let Ok(proxy_base) = std::env::var("DTAKO_STORAGE_HTTP_PROXY") {
            let bucket =
                std::env::var("DTAKO_R2_BUCKET").unwrap_or_else(|_| "dtako-uploads".to_string());
            tracing::info!(
                "Dtako storage: HTTP proxy at {} (bucket label={})",
                proxy_base,
                bucket
            );
            Some(Arc::new(rust_alc_api::storage::HttpProxyBackend::new(
                proxy_base, bucket,
            )) as Arc<dyn StorageBackend>)
        } else {
            std::env::var("DTAKO_R2_BUCKET").ok().map(|bucket| {
                let account_id = std::env::var("R2_ACCOUNT_ID")
                    .expect("R2_ACCOUNT_ID required for DTAKO_R2_BUCKET");
                let access_key =
                    std::env::var("DTAKO_R2_ACCESS_KEY").expect("DTAKO_R2_ACCESS_KEY required");
                let secret_key =
                    std::env::var("DTAKO_R2_SECRET_KEY").expect("DTAKO_R2_SECRET_KEY required");
                tracing::info!("Dtako storage: R2 (bucket={})", bucket);
                Arc::new(
                    rust_alc_api::storage::R2Backend::new(
                        bucket, account_id, access_key, secret_key, None,
                    )
                    .expect("Failed to init dtako R2 backend"),
                ) as Arc<dyn StorageBackend>
            })
        };

    // FCM (optional — disabled if FCM_PROJECT_ID is not set)
    let fcm = std::env::var("FCM_PROJECT_ID").ok().map(|project_id| {
        tracing::info!("FCM enabled (project={})", project_id);
        Arc::new(rust_alc_api::fcm::FcmSender::new(project_id))
            as Arc<dyn rust_alc_api::fcm::FcmSenderTrait>
    });

    let api_tokens = Arc::new(PgApiTokensRepository::new(pool.clone()));
    let auth = Arc::new(PgAuthRepository::new(pool.clone()));
    let bot_admin = Arc::new(PgBotAdminRepository::new(pool.clone()));
    let lw_client = Arc::new(alc_notify::clients::lineworks::LineworksBotClient::new());
    let bot_admin_ext: Arc<dyn alc_core::repository::bot_admin::BotAdminRepository> =
        bot_admin.clone();
    let car_inspections = Arc::new(PgCarInspectionRepository::new(pool.clone()));
    let carins_files = Arc::new(PgCarinsFilesRepository::new(pool.clone()));
    let carrying_items = Arc::new(PgCarryingItemsRepository::new(pool.clone()));
    let communication_items = Arc::new(PgCommunicationItemsRepository::new(pool.clone()));
    let daily_health = Arc::new(PgDailyHealthRepository::new(pool.clone()));
    let devices = Arc::new(PgDeviceRepository::new(pool.clone()));
    let driver_info = Arc::new(PgDriverInfoRepository::new(pool.clone()));
    let dtako_csv_proxy = Arc::new(PgDtakoCsvProxyRepository::new(pool.clone()));
    let dtako_daily_hours = Arc::new(PgDtakoDailyHoursRepository::new(pool.clone()));
    let dtako_logs = Arc::new(PgDtakoLogsRepository::new(pool.clone()));
    let dtako_drivers = Arc::new(PgDtakoDriversRepository::new(pool.clone()));
    let dtako_event_classifications =
        Arc::new(PgDtakoEventClassificationsRepository::new(pool.clone()));
    let dtako_operations = Arc::new(PgDtakoOperationsRepository::new(pool.clone()));
    let dtako_restraint_report = Arc::new(PgDtakoRestraintReportRepository::new(pool.clone()));
    let dtako_restraint_report_pdf =
        Arc::new(PgDtakoRestraintReportPdfRepository::new(pool.clone()));
    let dtako_scraper = Arc::new(PgDtakoScraperRepository::new(pool.clone()));
    let dtako_tickets = Arc::new(PgDtakoTicketsRepository::new(pool.clone()));
    let dtako_upload = Arc::new(PgDtakoUploadRepository::new(pool.clone()));
    let dtako_vehicles = Arc::new(PgDtakoVehiclesRepository::new(pool.clone()));
    let vehicle_settings_dumps = Arc::new(PgVehicleSettingsDumpsRepository::new(pool.clone()));
    let dtako_work_times = Arc::new(PgDtakoWorkTimesRepository::new(pool.clone()));
    let dtako_y_time_export = Arc::new(PgDtakoYTimeExportRepository::new(pool.clone()));
    let employees = Arc::new(PgEmployeeRepository::new(pool.clone()));
    let equipment_failures = Arc::new(PgEquipmentFailuresRepository::new(pool.clone()));
    let guidance_records = Arc::new(PgGuidanceRecordsRepository::new(pool.clone()));
    let health_baselines = Arc::new(PgHealthBaselinesRepository::new(pool.clone()));
    let items = Arc::new(PgItemsRepository::new(pool.clone()));
    let item_files = Arc::new(PgItemFilesRepository::new(pool.clone()));
    let measurements = Arc::new(PgMeasurementsRepository::new(pool.clone()));
    let nfc_tags = Arc::new(PgNfcTagRepository::new(pool.clone()));
    let sso_admin = Arc::new(PgSsoAdminRepository::new(pool.clone()));
    let tenant_users = Arc::new(PgTenantUsersRepository::new(pool.clone()));
    let tenko_call = Arc::new(PgTenkoCallRepository::new(pool.clone()));
    let tenko_records = Arc::new(PgTenkoRecordsRepository::new(pool.clone()));
    let tenko_schedules = Arc::new(PgTenkoSchedulesRepository::new(pool.clone()));
    let tenko_sessions = Arc::new(PgTenkoSessionRepository::new(pool.clone()));
    let tenko_webhooks = Arc::new(PgTenkoWebhooksRepository::new(pool.clone()));
    let timecard = Arc::new(PgTimecardRepository::new(pool.clone()));
    let notify_recipients = Arc::new(PgNotifyRecipientRepository::new(pool.clone()));
    let notify_groups = Arc::new(PgNotifyGroupRepository::new(pool.clone()));
    let notify_documents = Arc::new(PgNotifyDocumentRepository::new(pool.clone()));
    let notify_deliveries = Arc::new(PgNotifyDeliveryRepository::new(pool.clone()));
    let notify_line_config = Arc::new(PgNotifyLineConfigRepository::new(pool.clone()));
    let lineworks_channels = Arc::new(PgLineworksChannelsRepository::new(pool.clone()));
    let trouble_tickets = Arc::new(PgTroubleTicketsRepository::new(pool.clone()));
    let trouble_files = Arc::new(PgTroubleFilesRepository::new(pool.clone()));
    let trouble_workflow = Arc::new(PgTroubleWorkflowRepository::new(pool.clone()));
    let trouble_categories = Arc::new(PgTroubleCategoriesRepository::new(pool.clone()));
    let trouble_offices = Arc::new(PgTroubleOfficesRepository::new(pool.clone()));
    let trouble_progress_statuses =
        Arc::new(PgTroubleProgressStatusesRepository::new(pool.clone()));
    let trouble_notification_prefs =
        Arc::new(PgTroubleNotificationPrefsRepository::new(pool.clone()));
    let trouble_schedules = Arc::new(PgTroubleSchedulesRepository::new(pool.clone()));
    let trouble_tasks = Arc::new(PgTroubleTasksRepository::new(pool.clone()));
    let trouble_task_types = Arc::new(PgTroubleTaskTypesRepository::new(pool.clone()));
    let trouble_task_statuses = Arc::new(PgTroubleTaskStatusesRepository::new(pool.clone()));
    let trouble_field_layouts = Arc::new(PgTroubleFieldLayoutsRepository::new(pool.clone()));

    // notify 用 R2 (optional)
    let notify_storage: Option<Arc<dyn StorageBackend>> =
        std::env::var("NOTIFY_R2_BUCKET").ok().map(|bucket| {
            let account_id = std::env::var("R2_ACCOUNT_ID")
                .expect("R2_ACCOUNT_ID required for NOTIFY_R2_BUCKET");
            let access_key =
                std::env::var("NOTIFY_R2_ACCESS_KEY").expect("NOTIFY_R2_ACCESS_KEY required");
            let secret_key =
                std::env::var("NOTIFY_R2_SECRET_KEY").expect("NOTIFY_R2_SECRET_KEY required");
            tracing::info!("Notify storage: R2 (bucket={})", bucket);
            Arc::new(
                rust_alc_api::storage::R2Backend::new(
                    bucket, account_id, access_key, secret_key, None,
                )
                .expect("Failed to init notify R2 backend"),
            ) as Arc<dyn StorageBackend>
        });

    // trouble 用 R2 (optional)
    let trouble_storage: Option<Arc<dyn StorageBackend>> =
        std::env::var("TROUBLE_R2_BUCKET").ok().map(|bucket| {
            let account_id = std::env::var("R2_ACCOUNT_ID")
                .expect("R2_ACCOUNT_ID required for TROUBLE_R2_BUCKET");
            let access_key =
                std::env::var("TROUBLE_R2_ACCESS_KEY").expect("TROUBLE_R2_ACCESS_KEY required");
            let secret_key =
                std::env::var("TROUBLE_R2_SECRET_KEY").expect("TROUBLE_R2_SECRET_KEY required");
            tracing::info!("Trouble storage: R2 (bucket={})", bucket);
            Arc::new(
                rust_alc_api::storage::R2Backend::new(
                    bucket, account_id, access_key, secret_key, None,
                )
                .expect("Failed to init trouble R2 backend"),
            ) as Arc<dyn StorageBackend>
        });

    let webhook_service: Option<Arc<dyn rust_alc_api::webhook::WebhookService>> = {
        let wh_repo: Arc<dyn rust_alc_api::db::repository::WebhookRepository> = Arc::new(
            rust_alc_api::db::repository::PgWebhookRepository::new(pool.clone()),
        );
        let wh_http: Arc<dyn rust_alc_api::webhook::WebhookHttpClient> =
            Arc::new(rust_alc_api::webhook::ReqwestWebhookClient);
        Some(Arc::new(rust_alc_api::webhook::PgWebhookService::new(
            wh_repo, wh_http,
        )))
    };

    let tenko_state = alc_tenko::TenkoState {
        tenko_call,
        tenko_records,
        tenko_schedules,
        tenko_sessions,
        tenko_webhooks,
        daily_health,
        health_baselines,
        equipment_failures,
        driver_info,
        webhook: webhook_service.clone(),
    };

    // trouble ドメイン (Refs #513 Phase B) — AppState から分離した TroubleState。
    // 通知予約の発火は schedule-alarm DO worker (ippoan/nuxt-notify) に登録する
    // (Refs #550/#551)。SCHEDULE_ALARM_URL 未設定なら従来どおり登録なし (warn)。
    let trouble_alarm = alc_trouble::worker_alarm::WorkerAlarmClient::from_env();
    if trouble_alarm.is_none() {
        tracing::warn!(
            "SCHEDULE_ALARM_URL not set; trouble schedule alarms are disabled (Refs #551)"
        );
    }
    let trouble_state = alc_trouble::TroubleState {
        trouble_tickets,
        trouble_files,
        trouble_workflow,
        trouble_categories,
        trouble_offices,
        trouble_progress_statuses,
        trouble_notification_prefs,
        trouble_schedules,
        trouble_tasks,
        trouble_task_types,
        trouble_task_statuses,
        trouble_field_layouts,
        trouble_storage,
        webhook: webhook_service.clone(),
        cloud_tasks: trouble_alarm
            .map(|c| Arc::new(c) as Arc<dyn alc_trouble::cloud_tasks::CloudTasksClient>),
        notifier: Some(Arc::new(
            alc_trouble::notifier::LineworksTroubleNotifier::new(
                bot_admin_ext.clone(),
                lw_client.clone(),
            ),
        )),
        employees: Some(employees.clone()),
    };

    // camera ドメイン (Refs #556) — per-domain の alc-camera-api を廃止し monolith へ
    // 移植。障害の自動起票は TroubleDownTicketSink 経由で既存 trouble_tickets repo に
    // 配線する。CAMERA_DOWN_THRESHOLD 未設定なら DEFAULT_DOWN_THRESHOLD を使う。
    let camera_down_threshold: usize = std::env::var("CAMERA_DOWN_THRESHOLD")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n >= 1)
        .unwrap_or(DEFAULT_DOWN_THRESHOLD);
    let camera_state = CameraState {
        cameras: Arc::new(PgCamerasRepository::new(pool.clone())),
        down_ticket_sink: Arc::new(TroubleDownTicketSink(Arc::new(
            PgTroubleTicketsRepository::new(pool.clone()),
        ))),
        down_threshold: camera_down_threshold,
    };

    let state = AppState {
        pool: Some(pool.clone()),
        api_tokens,
        auth,
        bot_admin,
        car_inspections,
        carins_files,
        carrying_items,
        communication_items,
        devices,
        dtako_csv_proxy,
        dtako_daily_hours,
        dtako_logs,
        dtako_drivers,
        dtako_event_classifications,
        dtako_operations,
        dtako_restraint_report,
        dtako_restraint_report_pdf,
        dtako_scraper,
        dtako_tickets,
        dtako_upload,
        dtako_vehicles,
        dtako_work_times,
        dtako_y_time_export,
        vehicle_settings_dumps,
        employees,
        guidance_records,
        items,
        item_files,
        measurements,
        nfc_tags,
        sso_admin,
        tenant_users,
        timecard,
        storage,
        carins_storage,
        dtako_storage,
        fcm,
        notify_recipients,
        notify_groups,
        notify_documents,
        notify_deliveries,
        notify_line_config,
        lineworks_channels,
        notify_storage,
        redact_broadcaster: alc_core::redact_broadcast::RedactBroadcaster::from_env().map(Arc::new),
        realtime_bus: alc_core::realtime_bus::RealtimeBus::from_env().map(Arc::new),
        device_pair_client: rust_alc_api::device_pair_client::HttpDevicePairClient::from_env()
            .map(|c| Arc::new(c) as Arc<dyn rust_alc_api::device_pair_client::DevicePairClient>),
        webhook: webhook_service,
    };

    // 点呼予定超過チェック バックグラウンドタスク
    let overdue_repo: Arc<dyn rust_alc_api::db::repository::WebhookRepository> = Arc::new(
        rust_alc_api::db::repository::PgWebhookRepository::new(pool.clone()),
    );
    let overdue_tenko: Arc<dyn alc_tenko::overdue::TenkoOverdueRepository> = Arc::new(
        rust_alc_api::db::repository::PgTenkoOverdueRepository::new(pool),
    );
    let overdue_http: Arc<dyn rust_alc_api::webhook::WebhookHttpClient> =
        Arc::new(rust_alc_api::webhook::ReqwestWebhookClient);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            if let Err(e) = rust_alc_api::webhook::check_overdue_schedules(
                &*overdue_repo,
                &*overdue_tenko,
                &*overdue_http,
            )
            .await
            {
                tracing::error!("Overdue check failed: {e}");
            }
        }
    });

    // CORS: 任意オリジン全開放 (allow_origin(Any)) をやめ、ippoan 系フロントの
    // オリジンだけ許可する。Origin は `scheme://host[:port]` 形式 (path/userinfo
    // なし) なので host 抽出は単純分割で安全。許可 suffix は env
    // `CORS_ALLOWED_ORIGIN_SUFFIXES` (カンマ区切り) で上書き可。method は標準 verb に
    // 限定。header はカスタムヘッダ (X-Tenant-ID 等) が多いため Any を維持 (credentials
    // 不使用なので origin 制限が実質の境界)。Refs #389。
    let cors_suffixes: Vec<String> = std::env::var("CORS_ALLOWED_ORIGIN_SUFFIXES")
        .unwrap_or_else(|_| {
            "ippoan.org,mtamaramu.com,m-tama-ramu.workers.dev,localhost,127.0.0.1".to_string()
        })
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(move |origin, _req| {
            origin
                .to_str()
                .ok()
                .and_then(|o| o.split("://").nth(1))
                .map(|hostport| {
                    let host = hostport
                        .split('/')
                        .next()
                        .unwrap_or("")
                        .split(':')
                        .next()
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    cors_suffixes
                        .iter()
                        .any(|s| host == *s || host.ends_with(&format!(".{s}")))
                })
                .unwrap_or(false)
        }))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::PATCH,
            Method::OPTIONS,
        ])
        .allow_headers(Any);

    // email-receiver Worker からの internal ingest 経路 (`POST /api/dtako/tickets` 等)。
    // INTERNAL_SHARED_SECRET env が空なら mount しない (= caller が None を渡すことで
    // routes::internal_shared_secret_router が empty router を返す)。main.rs は coverage
    // 対象外なのでここで env を読み conditional logic を持つのが安全 (Refs
    // ippoan/email-receiver#1)。
    let internal_secret = std::env::var("INTERNAL_SHARED_SECRET")
        .ok()
        .filter(|s| !s.is_empty());
    if internal_secret.is_none() {
        tracing::warn!(
            "INTERNAL_SHARED_SECRET not set; /api/dtako/tickets internal routes are disabled"
        );
    }
    let api_router = rust_alc_api::routes::router(
        rust_alc_api::routes::internal_oidc_trust(),
        tenko_state,
        trouble_state,
        camera_state,
    )
    .merge(rust_alc_api::routes::internal_shared_secret_router(
        internal_secret,
    ));

    let app = Router::new()
        .nest("/api", api_router)
        .layer(Extension(bot_admin_ext))
        .layer(Extension(lw_client))
        .layer(axum::extract::DefaultBodyLimit::max(20 * 1024 * 1024)) // 20MB
        .layer(cors)
        // セキュリティレスポンスヘッダ (Refs #394 L-2)。JSON API のため XSS 面は
        // 小さいが、HSTS / nosniff / clickjacking 抑止は安価に効く。ハンドラが
        // 個別に設定した値は尊重する (if_not_present)。
        .layer(SetResponseHeaderLayer::if_not_present(
            STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!("listening on 0.0.0.0:{port}");
    axum::serve(listener, app).await?;

    Ok(())
}

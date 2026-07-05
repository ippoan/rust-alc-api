//! 点呼予定の overdue 検出 + webhook 通知 (alc-core::webhook から移設、Refs #513)。
//!
//! webhook 配信の generic 部分 (`WebhookRepository` / `deliver_webhook`) は
//! alc-core に残し、TenkoSchedule に依存する overdue 検出だけを本 crate が持つ。

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use alc_core::models::WebhookConfig;
use alc_core::repository::WebhookRepository;
use alc_core::webhook::{deliver_webhook, WebhookHttpClient};

use crate::models::TenkoSchedule;

/// overdue 検出に必要な tenko 側 read/write (Pg 実装は `crate::repo::overdue`)
#[async_trait]
pub trait TenkoOverdueRepository: Send + Sync {
    async fn find_overdue_configs(&self) -> Result<Vec<WebhookConfig>, sqlx::Error>;

    async fn find_overdue_schedules(
        &self,
        tenant_id: Uuid,
        overdue_minutes: i64,
    ) -> Result<Vec<TenkoSchedule>, sqlx::Error>;

    async fn get_employee_name(&self, employee_id: Uuid) -> Result<Option<String>, sqlx::Error>;

    async fn mark_overdue_notified(&self, schedule_id: Uuid) -> Result<(), sqlx::Error>;
}

/// 未完了予定の検出 + overdue通知 (バックグラウンドループから呼ばれる)
pub async fn check_overdue_schedules(
    repo: &dyn WebhookRepository,
    overdue: &dyn TenkoOverdueRepository,
    http: &dyn WebhookHttpClient,
) -> Result<(), anyhow::Error> {
    let overdue_minutes: i64 = std::env::var("TENKO_OVERDUE_MINUTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);

    let configs = overdue.find_overdue_configs().await?;

    for config in &configs {
        let overdue_schedules = overdue
            .find_overdue_schedules(config.tenant_id, overdue_minutes)
            .await?;

        for schedule in &overdue_schedules {
            let employee_name = overdue.get_employee_name(schedule.employee_id).await?;

            let minutes = (Utc::now() - schedule.scheduled_at).num_minutes();

            let payload = serde_json::json!({
                "event": "tenko_overdue",
                "timestamp": Utc::now(),
                "tenant_id": config.tenant_id,
                "data": {
                    "schedule_id": schedule.id,
                    "employee_id": schedule.employee_id,
                    "employee_name": employee_name.unwrap_or_default(),
                    "scheduled_at": schedule.scheduled_at,
                    "minutes_overdue": minutes,
                    "responsible_manager_name": schedule.responsible_manager_name,
                    "tenko_type": schedule.tenko_type,
                }
            });

            overdue.mark_overdue_notified(schedule.id).await?;

            let _ = deliver_webhook(repo, http, config, "tenko_overdue", &payload).await;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct MockRepo {
        deliveries: Mutex<Vec<(String, i32, bool)>>,
        overdue_configs: Vec<WebhookConfig>,
        overdue_schedules: Vec<TenkoSchedule>,
        employee_name: Option<String>,
        notified: Mutex<Vec<Uuid>>,
    }

    impl MockRepo {
        fn new() -> Self {
            Self {
                deliveries: Mutex::new(Vec::new()),
                overdue_configs: Vec::new(),
                overdue_schedules: Vec::new(),
                employee_name: None,
                notified: Mutex::new(Vec::new()),
            }
        }

        fn with_overdue(
            mut self,
            configs: Vec<WebhookConfig>,
            schedules: Vec<TenkoSchedule>,
        ) -> Self {
            self.overdue_configs = configs;
            self.overdue_schedules = schedules;
            self
        }

        fn with_employee_name(mut self, name: Option<String>) -> Self {
            self.employee_name = name;
            self
        }
    }

    #[async_trait]
    impl WebhookRepository for MockRepo {
        async fn find_config(
            &self,
            _tenant_id: Uuid,
            _event_type: &str,
        ) -> Result<Option<WebhookConfig>, sqlx::Error> {
            Ok(None)
        }

        async fn record_delivery(
            &self,
            _tenant_id: Uuid,
            _config_id: Uuid,
            event_type: &str,
            _payload: &serde_json::Value,
            _status_code: Option<i32>,
            _response_body: Option<&str>,
            attempt: i32,
            success: bool,
        ) -> Result<(), sqlx::Error> {
            self.deliveries
                .lock()
                .unwrap()
                .push((event_type.to_string(), attempt, success));
            Ok(())
        }
    }

    #[async_trait]
    impl TenkoOverdueRepository for MockRepo {
        async fn find_overdue_configs(&self) -> Result<Vec<WebhookConfig>, sqlx::Error> {
            Ok(self.overdue_configs.clone())
        }

        async fn find_overdue_schedules(
            &self,
            _tenant_id: Uuid,
            _overdue_minutes: i64,
        ) -> Result<Vec<TenkoSchedule>, sqlx::Error> {
            Ok(self.overdue_schedules.clone())
        }

        async fn get_employee_name(
            &self,
            _employee_id: Uuid,
        ) -> Result<Option<String>, sqlx::Error> {
            Ok(self.employee_name.clone())
        }

        async fn mark_overdue_notified(&self, schedule_id: Uuid) -> Result<(), sqlx::Error> {
            self.notified.lock().unwrap().push(schedule_id);
            Ok(())
        }
    }

    struct MockHttp;

    #[async_trait]
    impl WebhookHttpClient for MockHttp {
        async fn deliver(
            &self,
            _url: &str,
            _event_type: &str,
            _payload: &serde_json::Value,
            _secret: Option<&str>,
        ) -> Result<(Option<i32>, Option<String>, bool), anyhow::Error> {
            Ok((Some(200), Some("ok".to_string()), true))
        }
    }

    fn make_config() -> WebhookConfig {
        WebhookConfig {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            event_type: "tenko_overdue".to_string(),
            url: "https://example.com/webhook".to_string(),
            secret: None,
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn make_schedule(tenant_id: Uuid) -> TenkoSchedule {
        TenkoSchedule {
            id: Uuid::new_v4(),
            tenant_id,
            employee_id: Uuid::new_v4(),
            tenko_type: "pre_operation".to_string(),
            responsible_manager_name: "Manager A".to_string(),
            scheduled_at: Utc::now() - chrono::Duration::hours(2),
            instruction: None,
            consumed: false,
            consumed_by_session_id: None,
            overdue_notified_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn test_check_overdue_no_configs() {
        let repo = MockRepo::new();
        let http = MockHttp;

        let result = check_overdue_schedules(&repo, &repo, &http).await;

        assert!(result.is_ok());
        assert!(repo.notified.lock().unwrap().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn test_check_overdue_with_schedules() {
        let config = make_config();
        let tenant_id = config.tenant_id;
        let schedule = make_schedule(tenant_id);
        let schedule_id = schedule.id;

        let repo = MockRepo::new()
            .with_overdue(vec![config], vec![schedule])
            .with_employee_name(Some("Taro Yamada".to_string()));
        let http = MockHttp;

        let result = check_overdue_schedules(&repo, &repo, &http).await;

        assert!(result.is_ok());
        let notified = repo.notified.lock().unwrap();
        assert_eq!(notified.len(), 1);
        assert_eq!(notified[0], schedule_id);
        let deliveries = repo.deliveries.lock().unwrap();
        assert_eq!(deliveries.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn test_check_overdue_employee_name_none() {
        let config = make_config();
        let tenant_id = config.tenant_id;
        let schedule = make_schedule(tenant_id);

        let repo = MockRepo::new()
            .with_overdue(vec![config], vec![schedule])
            .with_employee_name(None);
        let http = MockHttp;

        let result = check_overdue_schedules(&repo, &repo, &http).await;

        assert!(result.is_ok());
        assert_eq!(repo.notified.lock().unwrap().len(), 1);
    }
}

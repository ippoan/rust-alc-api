//! schedule-alarm DO worker (ippoan/nuxt-notify `workers/schedule-alarm`) への
//! alarm 登録/解除 client。`CloudTasksClient` trait の Cloud Tasks 代替実装
//! (Refs #550 — Cloud Tasks は配線せず DO Alarm 方式を採用、#551)。
//!
//! 認証は既存 `INTERNAL_SHARED_SECRET` の再利用 (`X-Internal-Shared-Secret` header、
//! worker 側は同じ CF Secrets Store entry を bind)。発火経路は
//! worker → auth-worker `/alc-internal-proxy` → 本 API の
//! `POST /api/internal/trouble/schedules/{id}/fire` (冪等)。

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::cloud_tasks::{CloudTasksClient, CloudTasksError};

pub struct WorkerAlarmClient {
    client: reqwest::Client,
    /// wiremock でテストできるよう URL は const にせず struct フィールドで持つ。
    base_url: String,
    secret: String,
}

impl WorkerAlarmClient {
    pub fn new(base_url: String, secret: String) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            base_url,
            secret,
        }
    }

    /// `SCHEDULE_ALARM_URL` と `INTERNAL_SHARED_SECRET` が両方非空なら `Some`。
    /// 未設定なら `None` (= 従来どおり alarm 登録なし、呼び出し側が warn を出す)。
    pub fn from_env() -> Option<Self> {
        Self::from_env_lookup(|k| std::env::var(k).ok())
    }

    fn from_env_lookup<F: Fn(&str) -> Option<String>>(getter: F) -> Option<Self> {
        let base_url = getter("SCHEDULE_ALARM_URL").filter(|s| !s.is_empty())?;
        let secret = getter("INTERNAL_SHARED_SECRET").filter(|s| !s.is_empty())?;
        Some(Self::new(base_url, secret))
    }

    fn alarm_url(&self, schedule_id: Uuid) -> String {
        format!(
            "{}/alarms/{}",
            self.base_url.trim_end_matches('/'),
            schedule_id
        )
    }
}

#[async_trait]
impl CloudTasksClient for WorkerAlarmClient {
    async fn create_task(
        &self,
        schedule_id: Uuid,
        scheduled_at: DateTime<Utc>,
    ) -> Result<String, CloudTasksError> {
        let resp = self
            .client
            .put(self.alarm_url(schedule_id))
            .header("X-Internal-Shared-Secret", &self.secret)
            .json(&serde_json::json!({ "fire_at": scheduled_at.to_rfc3339() }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(CloudTasksError::Api { status, body });
        }

        // cancel 時に DELETE 先を復元できる相対パスを task_name として保存する。
        Ok(format!("alarms/{schedule_id}"))
    }

    async fn delete_task(&self, task_name: &str) -> Result<(), CloudTasksError> {
        // task_name は create_task が返した "alarms/{uuid}"。
        let url = format!("{}/{}", self.base_url.trim_end_matches('/'), task_name);
        let resp = self
            .client
            .delete(url)
            .header("X-Internal-Shared-Secret", &self.secret)
            .send()
            .await?;

        // 404 は既に消えている (worker 側 DELETE は冪等) ので成功扱い。
        if !resp.status().is_success() && resp.status().as_u16() != 404 {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(CloudTasksError::Api { status, body });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn getter<'a>(map: &'a HashMap<&'a str, &'a str>) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| map.get(k).map(|v| v.to_string())
    }

    #[test]
    fn from_env_lookup_returns_some_when_both_set() {
        let map = HashMap::from([
            ("SCHEDULE_ALARM_URL", "https://alarm.example"),
            ("INTERNAL_SHARED_SECRET", "s3cret"),
        ]);
        assert!(WorkerAlarmClient::from_env_lookup(getter(&map)).is_some());
    }

    #[test]
    fn from_env_lookup_returns_none_when_missing_or_empty() {
        for map in [
            HashMap::from([("SCHEDULE_ALARM_URL", "https://alarm.example")]),
            HashMap::from([("INTERNAL_SHARED_SECRET", "s3cret")]),
            HashMap::from([
                ("SCHEDULE_ALARM_URL", ""),
                ("INTERNAL_SHARED_SECRET", "s3cret"),
            ]),
            HashMap::from([
                ("SCHEDULE_ALARM_URL", "https://alarm.example"),
                ("INTERNAL_SHARED_SECRET", ""),
            ]),
        ] {
            assert!(WorkerAlarmClient::from_env_lookup(getter(&map)).is_none());
        }
    }

    #[tokio::test]
    async fn create_task_puts_alarm_with_secret_and_returns_task_name() {
        let server = MockServer::start().await;
        let id = Uuid::parse_str("61cf27f0-b192-4ca4-a608-1cc1b24f45c3").unwrap();
        let at = chrono::DateTime::parse_from_rfc3339("2026-07-10T04:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);

        Mock::given(method("PUT"))
            .and(path(format!("/alarms/{id}")))
            .and(header("X-Internal-Shared-Secret", "s3cret"))
            .and(body_json(serde_json::json!({ "fire_at": at.to_rfc3339() })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "scheduled": true
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = WorkerAlarmClient::new(server.uri(), "s3cret".into());
        let task_name = client.create_task(id, at).await.unwrap();
        assert_eq!(task_name, format!("alarms/{id}"));
    }

    #[tokio::test]
    async fn create_task_maps_non_2xx_to_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let client = WorkerAlarmClient::new(server.uri(), "wrong".into());
        let err = client
            .create_task(Uuid::new_v4(), Utc::now())
            .await
            .unwrap_err();
        match err {
            CloudTasksError::Api { status, .. } => assert_eq!(status, 401),
            other => panic!("unexpected error: {other}"),
        }
    }

    #[tokio::test]
    async fn delete_task_deletes_by_task_name_and_accepts_404() {
        let server = MockServer::start().await;
        let id = Uuid::new_v4();

        Mock::given(method("DELETE"))
            .and(path(format!("/alarms/{id}")))
            .and(header("X-Internal-Shared-Secret", "s3cret"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        let client = WorkerAlarmClient::new(server.uri(), "s3cret".into());
        client.delete_task(&format!("alarms/{id}")).await.unwrap();

        // 404 も成功扱い (冪等)
        let server2 = MockServer::start().await;
        Mock::given(method("DELETE"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server2)
            .await;
        let client2 = WorkerAlarmClient::new(server2.uri(), "s3cret".into());
        client2.delete_task("alarms/whatever").await.unwrap();
    }

    #[tokio::test]
    async fn delete_task_maps_5xx_to_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;

        let client = WorkerAlarmClient::new(server.uri(), "s3cret".into());
        let err = client.delete_task("alarms/x").await.unwrap_err();
        match err {
            CloudTasksError::Api { status, .. } => assert_eq!(status, 500),
            other => panic!("unexpected error: {other}"),
        }
    }
}

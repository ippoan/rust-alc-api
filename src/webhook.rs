use chrono::Utc;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::PgPool;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// Webhook イベントを発火 (非同期で配信)
pub async fn fire_event(
    pool: &PgPool,
    tenant_id: Uuid,
    event_type: &str,
    payload: serde_json::Value,
) -> Result<(), anyhow::Error> {
    let mut conn = pool.acquire().await?;
    crate::db::tenant::set_current_tenant(&mut conn, &tenant_id.to_string()).await?;

    let config = sqlx::query_as::<_, crate::db::models::WebhookConfig>(
        "SELECT * FROM webhook_configs WHERE tenant_id = $1 AND event_type = $2 AND enabled = TRUE",
    )
    .bind(tenant_id)
    .bind(event_type)
    .fetch_optional(&mut *conn)
    .await?;

    let config = match config {
        Some(c) => c,
        None => return Ok(()), // 設定なし → 何もしない
    };

    let pool = pool.clone();
    let event_type = event_type.to_string();
    tokio::spawn(async move {
        if let Err(e) = deliver_webhook(&pool, &config, &event_type, &payload).await {
            tracing::error!("Webhook delivery failed: {e}");
        }
    });

    Ok(())
}

/// Webhook を配信 (リトライ付き)
async fn deliver_webhook(
    pool: &PgPool,
    config: &crate::db::models::WebhookConfig,
    event_type: &str,
    payload: &serde_json::Value,
) -> Result<(), anyhow::Error> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let delays = [1u64, 5, 25]; // 指数バックオフ

    for attempt in 1..=3 {
        let body = serde_json::to_string(payload)?;

        let mut req = client
            .post(&config.url)
            .header("Content-Type", "application/json")
            .header("X-Webhook-Event", event_type);

        // HMAC-SHA256 署名
        if let Some(ref secret) = config.secret {
            let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC key length");
            mac.update(body.as_bytes());
            let signature = hex::encode(mac.finalize().into_bytes());
            req = req.header("X-Webhook-Signature", format!("sha256={signature}"));
        }

        let resp = req.body(body).send().await;

        let (status_code, response_body, success) = match resp {
            Ok(r) => {
                let code = r.status().as_u16() as i32;
                let body = r.text().await.unwrap_or_default();
                let ok = (200..300).contains(&(code as u16 as usize));
                (Some(code), Some(body), ok)
            }
            Err(e) => {
                tracing::warn!("Webhook attempt {attempt} failed: {e}");
                (None, Some(e.to_string()), false)
            }
        };

        // 配信ログ記録
        let _ = record_delivery(
            pool,
            config.tenant_id,
            config.id,
            event_type,
            payload,
            status_code,
            response_body.as_deref(),
            attempt,
            success,
        )
        .await;

        if success {
            return Ok(());
        }

        if attempt < 3 {
            tokio::time::sleep(std::time::Duration::from_secs(delays[attempt as usize - 1])).await;
        }
    }

    Ok(())
}

async fn record_delivery(
    pool: &PgPool,
    tenant_id: Uuid,
    config_id: Uuid,
    event_type: &str,
    payload: &serde_json::Value,
    status_code: Option<i32>,
    response_body: Option<&str>,
    attempt: i32,
    success: bool,
) -> Result<(), sqlx::Error> {
    let mut conn = pool.acquire().await?;
    crate::db::tenant::set_current_tenant(&mut conn, &tenant_id.to_string()).await?;

    sqlx::query(
        r#"
        INSERT INTO webhook_deliveries (
            tenant_id, config_id, event_type, payload,
            status_code, response_body, attempt, delivered_at, success
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
    )
    .bind(tenant_id)
    .bind(config_id)
    .bind(event_type)
    .bind(payload)
    .bind(status_code)
    .bind(response_body)
    .bind(attempt)
    .bind(if success { Some(Utc::now()) } else { None })
    .bind(success)
    .execute(&mut *conn)
    .await?;

    Ok(())
}

/// 未完了予定の検出 + overdue通知 (バックグラウンドループから呼ばれる)
pub async fn check_overdue_schedules(pool: &PgPool) -> Result<(), anyhow::Error> {
    // overdue_minutes 環境変数 (デフォルト60分)
    let overdue_minutes: i64 = std::env::var("TENKO_OVERDUE_MINUTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);

    // webhook_configs に tenko_overdue が設定されているテナントを取得
    let configs = sqlx::query_as::<_, crate::db::models::WebhookConfig>(
        "SELECT * FROM webhook_configs WHERE event_type = 'tenko_overdue' AND enabled = TRUE",
    )
    .fetch_all(pool)
    .await?;

    for config in &configs {
        let mut conn = pool.acquire().await?;
        crate::db::tenant::set_current_tenant(&mut conn, &config.tenant_id.to_string()).await?;

        // 予定時刻 + overdue_minutes を過ぎた未消費・未通知の予定を検索
        let overdue_schedules = sqlx::query_as::<_, crate::db::models::TenkoSchedule>(
            r#"
            SELECT s.* FROM tenko_schedules s
            WHERE s.tenant_id = $1
              AND s.consumed = FALSE
              AND s.overdue_notified_at IS NULL
              AND s.scheduled_at + ($2 || ' minutes')::INTERVAL < NOW()
            "#,
        )
        .bind(config.tenant_id)
        .bind(overdue_minutes.to_string())
        .fetch_all(&mut *conn)
        .await?;

        for schedule in &overdue_schedules {
            // 乗務員名を取得
            let employee_name: Option<String> =
                sqlx::query_scalar("SELECT name FROM employees WHERE id = $1")
                    .bind(schedule.employee_id)
                    .fetch_optional(&mut *conn)
                    .await?;

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

            // 通知済みマーク
            sqlx::query("UPDATE tenko_schedules SET overdue_notified_at = NOW() WHERE id = $1")
                .bind(schedule.id)
                .execute(&mut *conn)
                .await?;

            // Webhook 配信
            let _ = deliver_webhook(pool, config, "tenko_overdue", &payload).await;
        }
    }

    Ok(())
}

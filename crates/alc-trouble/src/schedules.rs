use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::models::{CreateTroubleSchedule, TroubleSchedule, TroubleTicket};
use crate::TroubleState;
use alc_core::auth_middleware::TenantId;

pub fn tenant_router<S>() -> Router<S>
where
    TroubleState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/trouble/schedules", post(create_schedule))
        .route(
            "/trouble/tickets/{ticket_id}/schedules",
            get(list_schedules),
        )
        .route(
            "/trouble/schedules/{id}",
            axum::routing::delete(cancel_schedule),
        )
}

/// スケジューラ (Cloud Tasks 等) から呼ばれる fire ルートの internal 版 (#434 lockdown)。
/// `require_internal_jwt` (aud=alc-api-internal) 配下に mount する。スケジューラは Google OIDC を
/// 直接付けられないため、auth-worker (OIDC mint) 経由で `/api/internal/trouble/schedules/{id}/fire`
/// を叩く。現状 `cloud_tasks` は未配線 (None) なので caller は居ないが、lockdown 後に経路を
/// 開けておくための internal mount。bare public は廃止 (IAM + ルート両面で塞ぐ)。
pub fn internal_fire_router<S>() -> Router<S>
where
    TroubleState: axum::extract::FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new().route("/internal/trouble/schedules/{id}/fire", post(fire_schedule))
}

async fn create_schedule(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Json(body): Json<CreateTroubleSchedule>,
) -> Result<(StatusCode, Json<TroubleSchedule>), StatusCode> {
    let tenant_id = tenant.0 .0;

    // 30日先までの制限
    let max_future = chrono::Utc::now() + chrono::Duration::days(30);
    if body.scheduled_at > max_future {
        return Err(StatusCode::BAD_REQUEST);
    }
    if body.scheduled_at <= chrono::Utc::now() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if body.message.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if body.lineworks_user_ids.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let schedule = state
        .trouble_schedules
        .create(tenant_id, &body, None)
        .await
        .map_err(|e| {
            tracing::error!("create_schedule DB error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Cloud Tasks にタスク登録
    if let Some(ct) = &state.cloud_tasks {
        match ct.create_task(schedule.id, schedule.scheduled_at).await {
            Ok(task_name) => {
                let _ = state
                    .trouble_schedules
                    .set_cloud_task_name(tenant_id, schedule.id, &task_name)
                    .await;
            }
            Err(e) => {
                tracing::error!("Cloud Tasks create error: {e}");
                // タスク登録失敗してもスケジュール自体は保存済み
            }
        }
    }

    Ok((StatusCode::CREATED, Json(schedule)))
}

async fn list_schedules(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(ticket_id): Path<Uuid>,
) -> Result<Json<Vec<TroubleSchedule>>, StatusCode> {
    let schedules = state
        .trouble_schedules
        .list_by_ticket(tenant.0 .0, ticket_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(schedules))
}

async fn cancel_schedule(
    State(state): State<TroubleState>,
    tenant: axum::Extension<TenantId>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let tenant_id = tenant.0 .0;

    // まず取得してcloud_task_nameを確認
    let schedule = state
        .trouble_schedules
        .get(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if schedule.status != "pending" {
        return Err(StatusCode::CONFLICT);
    }

    // Cloud Tasks からタスク削除
    if let (Some(ct), Some(task_name)) = (&state.cloud_tasks, &schedule.cloud_task_name) {
        if let Err(e) = ct.delete_task(task_name).await {
            tracing::error!("Cloud Tasks delete error: {e}");
        }
    }

    let cancelled = state
        .trouble_schedules
        .update_status(tenant_id, id, "cancelled")
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if cancelled {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// 発火メッセージ先頭に付けるチケット見出しを組み立てる (Refs #553)。
/// 空のフィールドは行ごと省略。日時は JST 表示。
fn build_ticket_heading(ticket: &TroubleTicket) -> String {
    // Cloud Run は UTC — 表示は常に JST (+09:00) 固定
    let jst = chrono::FixedOffset::east_opt(9 * 3600).expect("+09:00 is valid");

    let mut lines: Vec<String> = Vec::new();

    let when = ticket
        .occurred_at
        .map(|at| at.with_timezone(&jst).format("%Y-%m-%d %H:%M").to_string())
        .or_else(|| {
            ticket
                .occurred_date
                .map(|d| d.format("%Y-%m-%d").to_string())
        })
        .unwrap_or_default();
    let mut line1: Vec<&str> = Vec::new();
    let person = ticket.person_name.trim();
    if !person.is_empty() {
        line1.push(person);
    }
    if !when.is_empty() {
        line1.push(&when);
    }
    if !line1.is_empty() {
        lines.push(line1.join("  "));
    }

    let org = match (ticket.company_name.trim(), ticket.office_name.trim()) {
        ("", "") => String::new(),
        (c, "") => c.to_string(),
        ("", o) => o.to_string(),
        (c, o) => format!("{c}/{o}"),
    };
    let mut line2: Vec<&str> = Vec::new();
    if !org.is_empty() {
        line2.push(&org);
    }
    let location = ticket.location.trim();
    if !location.is_empty() {
        line2.push(location);
    }
    if !line2.is_empty() {
        lines.push(line2.join(" | "));
    }

    lines.join("\n")
}

/// 発火する LINE WORKS メッセージ本体を組み立てる (Refs #553)。
/// チケットが取得できない場合 (削除済み等) は見出しなしで本文のみ。
/// チケット URL は入れない: LINE (WORKS) のアプリ内ブラウザで開くと Google
/// OAuth が 403 disallowed_useragent でブロックされ、必ずアクセスエラーに
/// なるため (Refs #553 フィードバック)。
fn build_fire_message(ticket: Option<&TroubleTicket>, message: &str) -> String {
    let Some(ticket) = ticket else {
        return message.to_string();
    };

    let mut out = String::new();
    let heading = build_ticket_heading(ticket);
    if !heading.is_empty() {
        out.push_str(&heading);
        out.push_str("\n\n");
    }
    out.push_str(message);
    out
}

/// Cloud Tasks から呼ばれる fire エンドポイント
async fn fire_schedule(
    State(state): State<TroubleState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    // RLS バイパスで取得
    let schedule = state
        .trouble_schedules
        .get_for_fire(id)
        .await
        .map_err(|e| {
            tracing::error!("fire_schedule get error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    if schedule.status != "pending" {
        return Ok(StatusCode::OK);
    }

    // 見出し用のチケット取得。fire は RLS バイパス経路なので schedule.tenant_id で
    // テナントを明示する (TenantConn 経由、bypass getter は増やさない)。
    // 取得失敗 (削除済み等) でも通知自体は落とさず本文のみ送る (Refs #553)。
    let ticket = match state
        .trouble_tickets
        .get(schedule.tenant_id, schedule.ticket_id)
        .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("fire_schedule ticket fetch error (heading omitted): {e}");
            None
        }
    };
    let message = build_fire_message(ticket.as_ref(), &schedule.message);

    // 通知送信
    if let Some(notifier) = &state.notifier {
        notifier
            .notify(
                schedule.tenant_id,
                "trouble_schedule",
                &message,
                &schedule.lineworks_user_ids,
            )
            .await;
    }

    // 送信済みマーク
    let _ = state.trouble_schedules.mark_sent(id).await;

    Ok(StatusCode::OK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn ticket_fixture() -> TroubleTicket {
        TroubleTicket {
            id: Uuid::parse_str("61cf27f0-0000-0000-0000-000000000000").unwrap(),
            tenant_id: Uuid::nil(),
            ticket_no: 1,
            category: "事故".to_string(),
            title: "テスト".to_string(),
            // JST 2026-05-19 07:00 = UTC 2026-05-18 22:00
            occurred_at: Some(Utc.with_ymd_and_hms(2026, 5, 18, 22, 0, 0).unwrap()),
            occurred_date: None,
            company_name: "大石運輸".to_string(),
            office_name: "本社".to_string(),
            department: String::new(),
            person_name: "松江 寛人".to_string(),
            person_id: None,
            person_is_external: false,
            registration_number: String::new(),
            location: "本社整備工場前".to_string(),
            description: String::new(),
            status_id: None,
            assigned_to: None,
            progress_notes: String::new(),
            allowance: String::new(),
            damage_amount: None,
            compensation_amount: None,
            confirmation_notice: String::new(),
            disciplinary_content: String::new(),
            disciplinary_action: String::new(),
            disciplinary_committee: String::new(),
            road_service_cost: None,
            counterparty: String::new(),
            counterparty_insurance: String::new(),
            counterparty_vehicle: String::new(),
            custom_fields: serde_json::json!({}),
            due_date: None,
            overdue_notified_at: None,
            created_by: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted_at: None,
        }
    }

    #[test]
    fn heading_full_fields_with_jst_datetime() {
        let heading = build_ticket_heading(&ticket_fixture());
        assert_eq!(
            heading,
            "松江 寛人  2026-05-19 07:00\n大石運輸/本社 | 本社整備工場前"
        );
    }

    #[test]
    fn heading_falls_back_to_occurred_date_when_no_datetime() {
        let mut t = ticket_fixture();
        t.occurred_at = None;
        t.occurred_date = Some(chrono::NaiveDate::from_ymd_opt(2026, 5, 19).unwrap());
        let heading = build_ticket_heading(&t);
        assert!(heading.starts_with("松江 寛人  2026-05-19\n"));
    }

    #[test]
    fn heading_omits_empty_fields_per_line() {
        let mut t = ticket_fixture();
        t.person_name = String::new();
        t.occurred_at = None;
        // 1 行目 (対象者/日時) は丸ごと省略、2 行目のみ
        assert_eq!(build_ticket_heading(&t), "大石運輸/本社 | 本社整備工場前");

        let mut t2 = ticket_fixture();
        t2.company_name = String::new();
        t2.office_name = String::new();
        t2.location = String::new();
        assert_eq!(build_ticket_heading(&t2), "松江 寛人  2026-05-19 07:00");

        let mut t3 = ticket_fixture();
        t3.office_name = String::new();
        assert_eq!(
            build_ticket_heading(&t3),
            "松江 寛人  2026-05-19 07:00\n大石運輸 | 本社整備工場前"
        );

        let mut t4 = ticket_fixture();
        t4.location = String::new();
        assert_eq!(
            build_ticket_heading(&t4),
            "松江 寛人  2026-05-19 07:00\n大石運輸/本社"
        );
    }

    #[test]
    fn heading_empty_when_all_fields_empty() {
        let mut t = ticket_fixture();
        t.person_name = String::new();
        t.occurred_at = None;
        t.company_name = String::new();
        t.office_name = String::new();
        t.location = String::new();
        assert_eq!(build_ticket_heading(&t), "");
        // 見出しが空でも本文は素のまま (先頭に空行を作らない)
        assert_eq!(build_fire_message(Some(&t), "本文です"), "本文です");
    }

    #[test]
    fn fire_message_prepends_heading() {
        let t = ticket_fixture();
        let msg = build_fire_message(Some(&t), "本文です");
        assert_eq!(
            msg,
            "松江 寛人  2026-05-19 07:00\n大石運輸/本社 | 本社整備工場前\n\n本文です"
        );
    }

    #[test]
    fn fire_message_never_contains_ticket_url() {
        // LINE アプリ内ブラウザで開くと Google OAuth が 403 disallowed_useragent に
        // なるため、チケット URL は一切入れない (Refs #553 フィードバック)
        let t = ticket_fixture();
        let msg = build_fire_message(Some(&t), "本文");
        assert!(!msg.contains("http"));
        assert!(!msg.contains("/tickets/"));
    }

    #[test]
    fn fire_message_body_only_when_ticket_missing() {
        // チケット削除済み等で取得できない場合は見出しを付けない
        assert_eq!(build_fire_message(None, "本文です"), "本文です");
    }
}

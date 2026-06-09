//! ヘルスログ記録 + 連続失敗判定 → alc-trouble 自動起票 usecase (Refs #345)。
//!
//! Cloud Run の CPU throttling 対策として `tokio::spawn` の background 化はせず、
//! health-log POST のリクエスト内で同期的に判定・起票する (1 query 数件で軽量)。

use uuid::Uuid;

use alc_core::models::CreateTroubleTicket;
use alc_core::repository::cameras::{CameraHealthLog, CreateCameraHealthLog};
use alc_core::repository::{CamerasRepository, TroubleTicketsRepository};

/// camera の障害自動起票で使う trouble category。
const CAMERA_TROUBLE_CATEGORY: &str = "その他";

/// ヘルスログを 1 件記録し、必要なら障害の自動起票 / 復旧リンク解除を行う。
///
/// - `alive == false` が連続 `down_threshold` 回続き、かつ未解決の自動 ticket が
///   無ければ alc-trouble に起票して `cameras.active_down_ticket_id` にリンクする。
/// - `alive == true` (復旧) でリンクがあればリンクをクリアする (ticket は自動
///   クローズせず手動クローズに委ねる)。
///
/// 起票やステータス更新で失敗しても **ログ記録自体は成功扱い** で返す
/// (監視ログの欠損を避けるため、副作用の失敗は warning に留める)。
pub async fn record_health_and_maybe_ticket(
    cameras: &dyn CamerasRepository,
    tickets: &dyn TroubleTicketsRepository,
    tenant_id: Uuid,
    camera_id: Uuid,
    input: &CreateCameraHealthLog,
    down_threshold: usize,
) -> Result<CameraHealthLog, sqlx::Error> {
    let log = cameras
        .insert_health_log(tenant_id, camera_id, input)
        .await?;

    // カメラマスタが消えている等で取れなければ副作用はスキップ。
    let camera = match cameras.get(tenant_id, camera_id).await? {
        Some(c) => c,
        None => return Ok(log),
    };

    if input.alive {
        // 復旧: 自動起票リンクがあればクリア (ticket は手動クローズ)。
        if camera.active_down_ticket_id.is_some() {
            if let Err(e) = cameras
                .set_active_down_ticket(tenant_id, camera_id, None)
                .await
            {
                tracing::warn!(
                    camera_id = %camera_id,
                    tenant_id = %tenant_id,
                    error = %e,
                    "camera recovered but failed to clear active_down_ticket"
                );
            }
        }
        return Ok(log);
    }

    // down 系: 既に未解決 ticket があれば冪等に何もしない。
    if camera.active_down_ticket_id.is_some() {
        return Ok(log);
    }

    // 直近 down_threshold 件が全て alive=false なら連続失敗とみなす。
    let recent = cameras
        .recent_health_logs(tenant_id, camera_id, down_threshold as i64)
        .await?;
    let consecutive_down = recent.len() >= down_threshold && recent.iter().all(|l| !l.alive);
    if !consecutive_down {
        return Ok(log);
    }

    let ticket_input = build_camera_down_ticket(&camera.name, &camera.ip, camera_id, &log);
    match tickets.create(tenant_id, &ticket_input, None, None).await {
        Ok(ticket) => {
            if let Err(e) = cameras
                .set_active_down_ticket(tenant_id, camera_id, Some(ticket.id))
                .await
            {
                tracing::warn!(
                    camera_id = %camera_id,
                    ticket_id = %ticket.id,
                    error = %e,
                    "auto-ticketed camera down but failed to link active_down_ticket"
                );
            }
            tracing::info!(
                camera_id = %camera_id,
                tenant_id = %tenant_id,
                ticket_id = %ticket.id,
                "camera down auto-ticketed"
            );
        }
        Err(e) => {
            tracing::warn!(
                camera_id = %camera_id,
                tenant_id = %tenant_id,
                error = %e,
                "failed to auto-ticket camera down"
            );
        }
    }

    Ok(log)
}

/// 自動起票する trouble ticket のペイロードを組み立てる (純粋関数、テスト容易)。
pub fn build_camera_down_ticket(
    camera_name: &str,
    camera_ip: &str,
    camera_id: Uuid,
    log: &CameraHealthLog,
) -> CreateTroubleTicket {
    let title = format!("監視カメラ異常: {camera_name}");
    let description = format!(
        "監視カメラ「{name}」({ip}) が連続して応答しません。\n直近エラー: {err}",
        name = camera_name,
        ip = camera_ip,
        err = log.error.as_deref().unwrap_or("(なし)"),
    );
    CreateTroubleTicket {
        category: CAMERA_TROUBLE_CATEGORY.to_string(),
        title: Some(title),
        occurred_at: Some(log.checked_at),
        occurred_date: None,
        company_name: None,
        office_name: None,
        department: None,
        person_name: None,
        person_id: None,
        person_is_external: None,
        registration_number: None,
        location: Some(camera_ip.to_string()),
        description: Some(description),
        assigned_to: None,
        damage_amount: None,
        compensation_amount: None,
        road_service_cost: None,
        counterparty: None,
        counterparty_insurance: None,
        // 自動起票である事を識別するマーカー (UI の出所表示 / 集計用)。
        custom_fields: Some(serde_json::json!({
            "source": "camera_auto",
            "camera_id": camera_id,
        })),
        due_date: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log_fixture(alive: bool, err: Option<&str>) -> CameraHealthLog {
        CameraHealthLog {
            id: 1,
            tenant_id: Uuid::new_v4(),
            camera_id: Uuid::new_v4(),
            alive,
            latency_ms: None,
            error: err.map(|s| s.to_string()),
            checked_at: chrono::Utc::now(),
            source_device_id: None,
        }
    }

    #[test]
    fn build_ticket_sets_camera_auto_marker_and_title() {
        let cid = Uuid::new_v4();
        let log = log_fixture(false, Some("timeout"));
        let t = build_camera_down_ticket("正門", "192.168.1.10", cid, &log);
        assert_eq!(t.category, "その他");
        assert_eq!(t.title.as_deref(), Some("監視カメラ異常: 正門"));
        assert_eq!(t.location.as_deref(), Some("192.168.1.10"));
        assert!(t.description.as_deref().unwrap().contains("timeout"));
        let cf = t.custom_fields.unwrap();
        assert_eq!(cf["source"], "camera_auto");
        assert_eq!(cf["camera_id"], serde_json::json!(cid));
    }

    #[test]
    fn build_ticket_handles_missing_error() {
        let log = log_fixture(false, None);
        let t = build_camera_down_ticket("裏口", "10.0.0.2", Uuid::new_v4(), &log);
        assert!(t.description.as_deref().unwrap().contains("(なし)"));
    }
}

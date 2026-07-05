//! ヘルスログ記録 + 連続失敗判定 → 障害自動起票 usecase (Refs #345)。
//!
//! Cloud Run の CPU throttling 対策として `tokio::spawn` の background 化はせず、
//! health-log POST のリクエスト内で同期的に判定・起票する (1 query 数件で軽量)。
//!
//! 起票先は camera 所有の port [`DownTicketSink`] に抽象化しており、alc-trouble の
//! 型には一切依存しない (Refs #513 Phase B)。trouble への配線 (adapter) は binary
//! (alc-camera-api) 側が持つ。

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use alc_core::repository::cameras::{CameraHealthLog, CreateCameraHealthLog};
use alc_core::repository::CamerasRepository;

/// camera 障害起票のペイロード (camera 所有、trouble の型に依存しない)。
/// trouble 側のフィールド (category / custom_fields マーカー等) への写像は
/// adapter (binary 側) の責務。
#[derive(Debug, Clone)]
pub struct CameraDownTicket {
    pub title: String,
    pub description: String,
    /// 現状はカメラ IP を入れる (trouble の location に写像される)。
    pub location: String,
    pub occurred_at: DateTime<Utc>,
    /// 自動起票の出所識別用 (adapter が custom_fields マーカーに写像する)。
    pub camera_id: Uuid,
}

/// camera 所有の起票 port。実装 (adapter) は binary 側で trouble に配線する。
#[async_trait]
pub trait DownTicketSink: Send + Sync {
    /// 起票して発行された ticket id を返す。
    async fn open_down_ticket(
        &self,
        tenant_id: Uuid,
        ticket: CameraDownTicket,
    ) -> Result<Uuid, sqlx::Error>;
}

/// ヘルスログを 1 件記録し、必要なら障害の自動起票 / 復旧リンク解除を行う。
///
/// - `alive == false` が連続 `down_threshold` 回続き、かつ未解決の自動 ticket が
///   無ければ起票して `cameras.active_down_ticket_id` にリンクする。
/// - `alive == true` (復旧) でリンクがあればリンクをクリアする (ticket は自動
///   クローズせず手動クローズに委ねる)。
///
/// 起票やステータス更新で失敗しても **ログ記録自体は成功扱い** で返す
/// (監視ログの欠損を避けるため、副作用の失敗は warning に留める)。
pub async fn record_health_and_maybe_ticket(
    cameras: &dyn CamerasRepository,
    tickets: &dyn DownTicketSink,
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
    match tickets.open_down_ticket(tenant_id, ticket_input).await {
        Ok(ticket_id) => {
            if let Err(e) = cameras
                .set_active_down_ticket(tenant_id, camera_id, Some(ticket_id))
                .await
            {
                tracing::warn!(
                    camera_id = %camera_id,
                    ticket_id = %ticket_id,
                    error = %e,
                    "auto-ticketed camera down but failed to link active_down_ticket"
                );
            }
            tracing::info!(
                camera_id = %camera_id,
                tenant_id = %tenant_id,
                ticket_id = %ticket_id,
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

/// 自動起票するペイロードを組み立てる (純粋関数、テスト容易)。
pub fn build_camera_down_ticket(
    camera_name: &str,
    camera_ip: &str,
    camera_id: Uuid,
    log: &CameraHealthLog,
) -> CameraDownTicket {
    let title = format!("監視カメラ異常: {camera_name}");
    let description = format!(
        "監視カメラ「{name}」({ip}) が連続して応答しません。\n直近エラー: {err}",
        name = camera_name,
        ip = camera_ip,
        err = log.error.as_deref().unwrap_or("(なし)"),
    );
    CameraDownTicket {
        title,
        description,
        location: camera_ip.to_string(),
        occurred_at: log.checked_at,
        camera_id,
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
    fn build_ticket_sets_title_and_location() {
        let cid = Uuid::new_v4();
        let log = log_fixture(false, Some("timeout"));
        let t = build_camera_down_ticket("正門", "192.168.1.10", cid, &log);
        assert_eq!(t.title, "監視カメラ異常: 正門");
        assert_eq!(t.location, "192.168.1.10");
        assert!(t.description.contains("timeout"));
        assert_eq!(t.camera_id, cid);
        assert_eq!(t.occurred_at, log.checked_at);
    }

    #[test]
    fn build_ticket_handles_missing_error() {
        let log = log_fixture(false, None);
        let t = build_camera_down_ticket("裏口", "10.0.0.2", Uuid::new_v4(), &log);
        assert!(t.description.contains("(なし)"));
    }
}

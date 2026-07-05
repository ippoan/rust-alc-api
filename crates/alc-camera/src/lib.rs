//! 監視カメラ死活管理ドメイン (Refs #345)。
//!
//! 事業所の ONVIF カメラ (Tapo 等) の死活監視。alc-app (拠点タブレット) が
//! ONVIF GetSystemDateAndTime を周期実行し、結果を `camera_health_logs` に記録。
//! 連続失敗を集約して alc-trouble に障害を自動起票する。
//!
//! - CRUD / health-log / status の HTTP layer は `handlers` (tenant スコープ)。
//! - 連続失敗判定 + 自動起票 usecase は `health`。camera 所有の port
//!   [`health::DownTicketSink`] にのみ依存し、trouble の型・crate には依存しない
//!   (Refs #513 Phase B)。trouble への adapter は binary (alc-camera-api) が持つ。

pub mod handlers;
pub mod health;
pub mod repo;

use std::sync::Arc;

use alc_core::repository::CamerasRepository;

pub use health::{CameraDownTicket, DownTicketSink};

/// down 判定に必要な連続失敗回数のデフォルト (約 15 分周期 × 3 = 45 分)。
pub const DEFAULT_DOWN_THRESHOLD: usize = 3;

/// camera-api 用の最小 State。
#[derive(Clone)]
pub struct CameraState {
    pub cameras: Arc<dyn CamerasRepository>,
    /// 自動起票先 (camera 所有 port)。trouble への adapter を binary が注入する。
    pub down_ticket_sink: Arc<dyn DownTicketSink>,
    /// 連続失敗 down 判定のしきい値。
    pub down_threshold: usize,
}

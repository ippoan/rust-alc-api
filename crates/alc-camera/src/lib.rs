//! 監視カメラ死活管理ドメイン (Refs #345)。
//!
//! 事業所の ONVIF カメラ (Tapo 等) の死活監視。alc-app (拠点タブレット) が
//! ONVIF GetSystemDateAndTime を周期実行し、結果を `camera_health_logs` に記録。
//! 連続失敗を集約して alc-trouble に障害を自動起票する。
//!
//! - CRUD / health-log / status の HTTP layer は `handlers` (tenant スコープ)。
//! - 連続失敗判定 + 自動起票 usecase は `health`。alc-core の
//!   `TroubleTicketsRepository` trait だけに依存し、alc-trouble 本体には依存しない
//!   (binary 側で Pg 実装を注入する)。

pub mod handlers;
pub mod health;
pub mod repo;

use std::sync::Arc;

use alc_core::repository::{CamerasRepository, TroubleTicketsRepository};

/// down 判定に必要な連続失敗回数のデフォルト (約 15 分周期 × 3 = 45 分)。
pub const DEFAULT_DOWN_THRESHOLD: usize = 3;

/// camera-api 用の最小 State。
#[derive(Clone)]
pub struct CameraState {
    pub cameras: Arc<dyn CamerasRepository>,
    /// 自動起票先。alc-trouble の Pg 実装を binary が注入する。
    pub trouble_tickets: Arc<dyn TroubleTicketsRepository>,
    /// 連続失敗 down 判定のしきい値。
    pub down_threshold: usize,
}

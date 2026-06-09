//! 監視カメラ死活管理ドメインの repository trait + 値型 (Refs #345)。
//!
//! `cameras` (マスタ) と `camera_health_logs` (ヘルスチェック結果) を扱う。
//! 連続失敗判定 → alc-trouble 自動起票の usecase は alc-camera 側に置き、
//! ここは永続化の抽象だけを定義する。

use async_trait::async_trait;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct Camera {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub office_id: Option<Uuid>,
    pub name: String,
    pub ip: String,
    pub onvif_port: i32,
    pub model: String,
    pub active: bool,
    /// down 検知で自動起票した未解決 ticket。down 中の重複起票防止に使う。
    pub active_down_ticket_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct CameraHealthLog {
    pub id: i64,
    pub tenant_id: Uuid,
    pub camera_id: Uuid,
    pub alive: bool,
    pub latency_ms: Option<i32>,
    pub error: Option<String>,
    pub checked_at: chrono::DateTime<chrono::Utc>,
    pub source_device_id: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CreateCamera {
    pub office_id: Option<Uuid>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub ip: String,
    pub onvif_port: Option<i32>,
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct UpdateCamera {
    pub office_id: Option<Uuid>,
    pub name: Option<String>,
    pub ip: Option<String>,
    pub onvif_port: Option<i32>,
    pub model: Option<String>,
    pub active: Option<bool>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CreateCameraHealthLog {
    pub alive: bool,
    pub latency_ms: Option<i32>,
    pub error: Option<String>,
    pub source_device_id: Option<String>,
}

/// 各カメラの最新ステータス集計 1 行 (`GET /cameras/status` 用)。
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct CameraStatusRow {
    pub camera_id: Uuid,
    pub name: String,
    pub active: bool,
    /// 直近ログの alive。ログが 1 件も無ければ NULL。
    pub last_alive: Option<bool>,
    pub last_latency_ms: Option<i32>,
    pub last_checked_at: Option<chrono::DateTime<chrono::Utc>>,
    /// 自動起票済みの未解決 ticket があれば。
    pub active_down_ticket_id: Option<Uuid>,
}

#[async_trait]
pub trait CamerasRepository: Send + Sync {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<Camera>, sqlx::Error>;

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Camera>, sqlx::Error>;

    async fn create(&self, tenant_id: Uuid, input: &CreateCamera) -> Result<Camera, sqlx::Error>;

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateCamera,
    ) -> Result<Option<Camera>, sqlx::Error>;

    /// 物理削除 (health_logs は ON DELETE CASCADE で消える)。
    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error>;

    async fn insert_health_log(
        &self,
        tenant_id: Uuid,
        camera_id: Uuid,
        input: &CreateCameraHealthLog,
    ) -> Result<CameraHealthLog, sqlx::Error>;

    /// 直近 `limit` 件を checked_at 降順で返す (連続失敗判定 / 管理画面表示の両用)。
    async fn recent_health_logs(
        &self,
        tenant_id: Uuid,
        camera_id: Uuid,
        limit: i64,
    ) -> Result<Vec<CameraHealthLog>, sqlx::Error>;

    /// 各カメラの最新ステータスを集計して返す。
    async fn statuses(&self, tenant_id: Uuid) -> Result<Vec<CameraStatusRow>, sqlx::Error>;

    /// 自動起票した未解決 ticket のリンクを設定 / クリア (冪等性管理)。
    async fn set_active_down_ticket(
        &self,
        tenant_id: Uuid,
        camera_id: Uuid,
        ticket_id: Option<Uuid>,
    ) -> Result<(), sqlx::Error>;
}

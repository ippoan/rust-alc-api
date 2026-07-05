//! FCM 送信の port (trait + error のみ)。
//!
//! 実装 (`FcmSender` = FCM v1 API + Cloud Run metadata server 認証) は
//! `alc_notify::fcm` に居る (port/adapter 分離、Refs #539)。AppState は
//! この trait object だけを保持し、合成は main.rs が行う。

/// FCM 送信 trait (テスト用モック対応)
#[async_trait::async_trait]
pub trait FcmSenderTrait: Send + Sync {
    async fn send_data_message(
        &self,
        fcm_token: &str,
        data: std::collections::HashMap<String, String>,
    ) -> Result<(), FcmError>;
}

#[derive(Debug, thiserror::Error)]
pub enum FcmError {
    #[error("FCM auth error: {0}")]
    Auth(String),
    #[error("FCM send error: {0}")]
    Send(String),
}

//! device pairing の port (trait + credential 型 + error のみ)。
//!
//! 実装 (`HttpDevicePairClient` = auth-worker `/device/pair-internal` を叩く
//! HTTP client、Refs #495) は `alc_devices::device_pair_client` に居る
//! (port/adapter 分離、Refs #539)。AppState はこの trait object だけを保持し、
//! 合成は main.rs が行う。

use async_trait::async_trait;
use uuid::Uuid;

/// auth-worker が発行した device credential。
///
/// `Debug` は手書きで `device_secret` を redact する (値を誤って log に出す
/// 事故防止。このリポジトリの「値を log に出さない」方針の防御的担保)。
#[derive(Clone, PartialEq, Eq)]
pub struct PairedCredential {
    pub device_id: String,
    pub device_secret: String,
}

impl std::fmt::Debug for PairedCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairedCredential")
            .field("device_id", &self.device_id)
            .field("device_secret", &"***")
            .finish()
    }
}

#[derive(Debug)]
pub enum DevicePairClientError {
    /// auth-worker への到達失敗 / 非 2xx / body parse 失敗。詳細は log にのみ出す。
    Upstream(String),
}

#[async_trait]
pub trait DevicePairClient: Send + Sync {
    async fn mint(
        &self,
        tenant_id: Uuid,
        label: &str,
    ) -> Result<PairedCredential, DevicePairClientError>;
}

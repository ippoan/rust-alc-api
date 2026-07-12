use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{HubMeasurementCreate, HubMeasurementsIngestResponse};

/// hub_measurements テーブルの抽象 (Refs #564)。
///
/// cf-alc-recorder Worker から internal shared-secret 経由で呼ばれる ingest 専用
/// (tenant スコープは X-Tenant-ID で渡る)。read 経路は現時点では持たない。
#[async_trait]
pub trait HubMeasurementsRepository: Send + Sync {
    /// バッチ insert。`UNIQUE (tenant_id, device_id, seq)` の衝突 (再送重複) は
    /// ON CONFLICT DO NOTHING でスキップし、duplicates として数える。
    async fn insert_batch(
        &self,
        tenant_id: Uuid,
        items: &[HubMeasurementCreate],
    ) -> Result<HubMeasurementsIngestResponse, sqlx::Error>;
}

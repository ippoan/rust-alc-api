use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{
    HubMeasurement, HubMeasurementCreate, HubMeasurementFilter, HubMeasurementsIngestResponse,
};

/// hub_measurements テーブルの抽象 (Refs #564 / read は #592)。
///
/// 書き込みは cf-alc-recorder Worker から internal shared-secret 経由で呼ばれる
/// ingest (tenant スコープは X-Tenant-ID で渡る)。読み出しはテナント認証付き
/// router の `GET /api/hub/measurements`。
#[async_trait]
pub trait HubMeasurementsRepository: Send + Sync {
    /// バッチ insert。`UNIQUE (tenant_id, device_id, seq)` の衝突 (再送重複) は
    /// ON CONFLICT DO NOTHING でスキップし、duplicates として数える。
    ///
    /// **payload は呼び出し側が凍結済みのものを渡す** — `kind="timecard"` の
    /// `employee_id` は ingest handler が insert の前に解決して入れる
    /// (Refs ippoan/alc-app-s3#134)。ここで再解決しないこと: 再送は
    /// ON CONFLICT DO NOTHING で弾かれ、**最初に入った payload が残る**のが
    /// 凍結の実体で、後から解決し直すと過去の打刻が動く。
    async fn insert_batch(
        &self,
        tenant_id: Uuid,
        items: &[HubMeasurementCreate],
    ) -> Result<HubMeasurementsIngestResponse, sqlx::Error>;

    /// tenant スコープの一覧 (`created_at DESC`)。`limit` は呼び出し側で clamp 済みの
    /// 実効値、`offset` は 0 以上を渡す。次ページ有無の判定に使えるよう、実装は
    /// **最大 `limit + 1` 件**を返してよい (切り詰めは呼び出し側)。
    async fn list(
        &self,
        tenant_id: Uuid,
        filter: &HubMeasurementFilter,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<HubMeasurement>, sqlx::Error>;
}

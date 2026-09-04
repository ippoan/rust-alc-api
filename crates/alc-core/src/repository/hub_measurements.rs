use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{HubMeasurement, HubMeasurementCreate, HubMeasurementFilter};

/// hub_measurements テーブルの抽象 (Refs #564 / read は #592)。
///
/// 書き込みは cf-alc-recorder Worker から internal shared-secret 経由で呼ばれる
/// ingest (tenant スコープは X-Tenant-ID で渡る)。読み出しはテナント認証付き
/// router の `GET /api/hub/measurements`。
#[async_trait]
pub trait HubMeasurementsRepository: Send + Sync {
    /// バッチ insert。`UNIQUE (tenant_id, device_id, seq)` の衝突 (再送重複) は
    /// ON CONFLICT DO NOTHING でスキップする。
    ///
    /// 戻り値は **`items` と同じ長さ・同じ順序**の「その行が新規に入ったか」。
    /// 件数への畳み込み (`inserted` / `duplicates`) は呼び出し側が行う。
    ///
    /// **なぜ件数ではなく行ごとに返すのか**: `kind="timecard"` の打刻中継
    /// (Refs ippoan/alc-app-s3#134) が「新規に入った行のときだけ打刻する」ために
    /// どの行が新規かを知る必要がある。端末は ack されるまで同じ seq を再送するので、
    /// ここが二重打刻を防ぐ唯一の関門になる。
    async fn insert_batch(
        &self,
        tenant_id: Uuid,
        items: &[HubMeasurementCreate],
    ) -> Result<Vec<bool>, sqlx::Error>;

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

use axum::{extract::State, http::StatusCode, routing::post, Extension, Json, Router};
use uuid::Uuid;

use alc_core::auth_middleware::TenantId;
use alc_core::models::{HubMeasurementCreate, HubMeasurementsIngestResponse};
use alc_core::AppState;

/// `STAGING_MODE=true` かどうか (alc-auth / alc-misc::staging と同判定)。
fn is_staging_mode() -> bool {
    std::env::var("STAGING_MODE")
        .map(|v| v == "true")
        .unwrap_or(false)
}

/// staging の揮発 DB では device credential に紐づく operator テナントが
/// cold start 毎に消え、device JWT 由来の tenant_id が dangling になって
/// `hub_measurements_tenant_id_fkey` 違反で 500 になる (ippoan/alc-app-s3#21
/// 実機 e2e で確認)。alc-auth::internal の `ensure_tenant_for_staging` と
/// 同方針で、STAGING_MODE 限定で tenant を冪等作成して救済する。
/// 本番では tenant が永続なので dangling は起きず、この救済は走らない (no-op)。
/// seed (staging/entrypoint.sh) への tenant ハードコード追記は不要になる。
async fn ensure_tenant_for_staging(state: &AppState, tenant_id: Uuid) -> Result<(), StatusCode> {
    if !is_staging_mode() {
        return Ok(());
    }
    state
        .auth
        .ensure_tenant_exists(tenant_id)
        .await
        .map_err(|e| {
            tracing::error!("hub_measurements ensure_tenant_exists error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// 受理する測定種別の allowlist (Refs #564 設計レビュー 2026-07-12)。
///
/// - temperature / blood_pressure … ble-medical-gateway 互換 JSON
/// - alcohol … CoreS3 が fc1200 プロトコルを端末上で解釈したパース済み測定値
/// - fc1200_raw … パース失敗時の hex パススルー fallback
/// - crash_log … CoreS3 の異常リセット復帰時レポート (reset reason + panic 前ログ、
///   Refs ippoan/alc-app-s3#43)
///
/// 将来の拡張 (timecard イベント等) はここに足す。DB 側に CHECK は張っていない
/// (migration 126 参照) ため、拡張はコード変更のみで済む。
pub const HUB_MEASUREMENT_KINDS: &[&str] = &[
    "temperature",
    "blood_pressure",
    "alcohol",
    "fc1200_raw",
    "crash_log",
];

/// 1 リクエストで受けるバッチの上限 (再送スパイクからの防御)。
const MAX_BATCH_ITEMS: usize = 500;

/// device_id の長さ上限 (auth-worker の device_id は URL-safe 短文字列)。
const MAX_DEVICE_ID_LEN: usize = 128;

/// cf-alc-recorder Worker から INTERNAL_SHARED_SECRET + X-Tenant-ID で叩く ingest。
/// `require_internal_shared_secret` middleware 配下に nest される想定。
pub fn internal_router() -> Router<AppState> {
    Router::new().route("/hub/measurements", post(ingest))
}

/// バッチ (配列) と単発 (object) の両方を受ける。
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum IngestBody {
    Batch(Vec<HubMeasurementCreate>),
    Single(HubMeasurementCreate),
}

impl IngestBody {
    fn into_items(self) -> Vec<HubMeasurementCreate> {
        match self {
            IngestBody::Batch(items) => items,
            IngestBody::Single(item) => vec![item],
        }
    }
}

/// item 単位の検証。エラーは詳細を返さず 400 に丸める (呼び出し元は内部 Worker のみ)。
fn validate(item: &HubMeasurementCreate) -> bool {
    !item.device_id.trim().is_empty()
        && item.device_id.len() <= MAX_DEVICE_ID_LEN
        && HUB_MEASUREMENT_KINDS.contains(&item.kind.as_str())
        && item.seq >= 0
}

async fn ingest(
    State(state): State<AppState>,
    tenant: Extension<TenantId>,
    Json(body): Json<IngestBody>,
) -> Result<(StatusCode, Json<HubMeasurementsIngestResponse>), StatusCode> {
    let items = body.into_items();
    if items.is_empty() || items.len() > MAX_BATCH_ITEMS {
        return Err(StatusCode::BAD_REQUEST);
    }
    if !items.iter().all(validate) {
        return Err(StatusCode::BAD_REQUEST);
    }
    ensure_tenant_for_staging(&state, tenant.0 .0).await?;
    let resp = state
        .hub_measurements
        .insert_batch(tenant.0 .0, &items)
        .await
        .map_err(|e| {
            tracing::error!("hub_measurements.insert_batch error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok((StatusCode::CREATED, Json(resp)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: &str, seq: i64, device_id: &str) -> HubMeasurementCreate {
        HubMeasurementCreate {
            device_id: device_id.to_string(),
            kind: kind.to_string(),
            seq,
            recorded_at_ms: None,
            payload: serde_json::json!({}),
        }
    }

    #[test]
    fn validate_accepts_allowlisted_kinds() {
        for kind in HUB_MEASUREMENT_KINDS {
            assert!(validate(&item(kind, 0, "dev-1")), "kind={kind}");
        }
    }

    #[test]
    fn validate_rejects_unknown_kind_empty_device_and_negative_seq() {
        assert!(!validate(&item("unknown", 0, "dev-1")));
        assert!(!validate(&item("alcohol", 0, "  ")));
        assert!(!validate(&item("alcohol", -1, "dev-1")));
        assert!(!validate(&item("alcohol", 0, &"x".repeat(129))));
    }

    #[test]
    fn ingest_body_accepts_single_and_batch() {
        let single: IngestBody =
            serde_json::from_str(r#"{"device_id":"d","kind":"alcohol","seq":1,"payload":{}}"#)
                .expect("single");
        assert_eq!(single.into_items().len(), 1);
        let batch: IngestBody = serde_json::from_str(
            r#"[{"device_id":"d","kind":"alcohol","seq":1,"payload":{}},
                {"device_id":"d","kind":"temperature","seq":2,"recorded_at_ms":1752300000000,"payload":{"value":36.5}}]"#,
        )
        .expect("batch");
        assert_eq!(batch.into_items().len(), 2);
    }
}

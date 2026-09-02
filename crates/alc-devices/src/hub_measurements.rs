use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Extension, Json, Router,
};
use uuid::Uuid;

use alc_core::auth_middleware::TenantId;
use alc_core::models::{
    HubMeasurementCreate, HubMeasurementFilter, HubMeasurementsIngestResponse,
    HubMeasurementsListResponse,
};
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
/// - license … CoreS3 が点呼開始時に読み取る免許証 IC (Refs ippoan/alc-app-s3#125)。
///   同じ session_id で測定と束ねて送られる。
///
/// 将来の拡張 (timecard イベント等) はここに足す。DB 側に CHECK は張っていない
/// (migration 126 参照) ため、拡張はコード変更のみで済む。
pub const HUB_MEASUREMENT_KINDS: &[&str] = &[
    "temperature",
    "blood_pressure",
    "alcohol",
    "fc1200_raw",
    "license",
];

/// 1 リクエストで受けるバッチの上限 (再送スパイクからの防御)。
const MAX_BATCH_ITEMS: usize = 500;

/// device_id の長さ上限 (auth-worker の device_id は URL-safe 短文字列)。
const MAX_DEVICE_ID_LEN: usize = 128;

/// session_id の長さ上限 (Refs ippoan/alc-app-s3#112)。端末が発番する短い文字列
/// (実装はセッション開始時の seq) なので、この余裕で足りる。
const MAX_SESSION_ID_LEN: usize = 64;

/// 一覧の既定件数と上限 (Refs #592)。payload は JSONB 素通しで 1 行が数百 byte〜
/// 数 KB になり得るので、上限は控えめに取る。
const DEFAULT_LIST_LIMIT: i64 = 50;
const MAX_LIST_LIMIT: i64 = 200;

/// cf-alc-recorder Worker から INTERNAL_SHARED_SECRET + X-Tenant-ID で叩く ingest。
/// `require_internal_shared_secret` middleware 配下に nest される想定。
pub fn internal_router() -> Router<AppState> {
    Router::new().route("/hub/measurements", post(ingest))
}

/// テナント認証付き (X-Tenant-ID) の閲覧経路 (Refs #592)。
///
/// ingest 用の [`internal_router`] とは**別 router**。あちらは cf-alc-recorder 専用の
/// shared-secret 経路なので、パスが同じでも混ぜない (認証方式が違う)。
pub fn tenant_router() -> Router<AppState> {
    Router::new().route("/hub/measurements", get(list))
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

/// session_id は端末由来 (untrusted) なので、長さと文字種を絞る。
/// None (点呼外の単発計測・旧ファーム) は正常値として通す。
fn valid_session_id(session_id: Option<&String>) -> bool {
    match session_id {
        None => true,
        Some(v) => {
            !v.is_empty()
                && v.len() <= MAX_SESSION_ID_LEN
                && v.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        }
    }
}

/// item 単位の検証。エラーは詳細を返さず 400 に丸める (呼び出し元は内部 Worker のみ)。
fn validate(item: &HubMeasurementCreate) -> bool {
    !item.device_id.trim().is_empty()
        && item.device_id.len() <= MAX_DEVICE_ID_LEN
        && HUB_MEASUREMENT_KINDS.contains(&item.kind.as_str())
        && item.seq >= 0
        && valid_session_id(item.session_id.as_ref())
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

/// clamp 済みの (limit, offset) を返す。負値・0・上限超えを安全側に丸める。
/// 呼び出し側の入力ミスで全件スキャンにならないよう、ここが唯一の関門。
fn clamp_paging(filter: &HubMeasurementFilter) -> (i64, i64) {
    let limit = filter
        .limit
        .unwrap_or(DEFAULT_LIST_LIMIT)
        .clamp(1, MAX_LIST_LIMIT);
    let offset = filter.offset.unwrap_or(0).max(0);
    (limit, offset)
}

/// `GET /api/hub/measurements` — tenant スコープの一覧 (created_at DESC)。
///
/// 絞り込みは device_id / kind / session_id / 期間 (from・to は created_at に対する閉区間)。
/// kind は allowlist 外を 400 で弾く (typo を無言の 0 件と区別できるようにする)。
/// session_id は 1 回の点呼を束ねて引くためのもの (Refs ippoan/alc-app-s3#112)。
async fn list(
    State(state): State<AppState>,
    tenant: Extension<TenantId>,
    Query(filter): Query<HubMeasurementFilter>,
) -> Result<Json<HubMeasurementsListResponse>, StatusCode> {
    if let Some(ref kind) = filter.kind {
        if !HUB_MEASUREMENT_KINDS.contains(&kind.as_str()) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    if !valid_session_id(filter.session_id.as_ref()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    if let (Some(from), Some(to)) = (filter.from, filter.to) {
        if from > to {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    let (limit, offset) = clamp_paging(&filter);

    let mut items = state
        .hub_measurements
        .list(tenant.0 .0, &filter, limit, offset)
        .await
        .map_err(|e| {
            tracing::error!("hub_measurements.list error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // repo は has_more 判定用に limit + 1 件まで返す。溢れた分はここで落とす。
    let has_more = items.len() as i64 > limit;
    items.truncate(limit as usize);

    Ok(Json(HubMeasurementsListResponse {
        items,
        limit,
        offset,
        has_more,
    }))
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
            session_id: None,
            payload: serde_json::json!({}),
        }
    }

    fn item_with_session(session_id: Option<&str>) -> HubMeasurementCreate {
        HubMeasurementCreate {
            session_id: session_id.map(str::to_string),
            ..item("alcohol", 1, "dev-1")
        }
    }

    #[test]
    fn validate_accepts_allowlisted_kinds() {
        for kind in HUB_MEASUREMENT_KINDS {
            assert!(validate(&item(kind, 0, "dev-1")), "kind={kind}");
        }
    }

    #[test]
    fn validate_accepts_license_kind() {
        assert!(validate(&item("license", 0, "dev-1")));
    }

    #[test]
    fn validate_rejects_unknown_kind_empty_device_and_negative_seq() {
        assert!(!validate(&item("unknown", 0, "dev-1")));
        assert!(!validate(&item("alcohol", 0, "  ")));
        assert!(!validate(&item("alcohol", -1, "dev-1")));
        assert!(!validate(&item("alcohol", 0, &"x".repeat(129))));
    }

    #[test]
    fn validate_accepts_well_formed_session_id_and_rejects_junk() {
        // None (点呼外の単発計測 / 旧ファーム) は正常値
        assert!(validate(&item_with_session(None)));
        assert!(validate(&item_with_session(Some("s42"))));
        assert!(validate(&item_with_session(Some("boot-1234_7"))));
        // 空・長すぎ・記号混じりは弾く (端末由来の untrusted 値)
        assert!(!validate(&item_with_session(Some(""))));
        assert!(!validate(&item_with_session(Some(&"x".repeat(65)))));
        assert!(!validate(&item_with_session(Some("s 42"))));
        assert!(!validate(&item_with_session(Some("s/42"))));
    }

    fn filter(limit: Option<i64>, offset: Option<i64>) -> HubMeasurementFilter {
        HubMeasurementFilter {
            limit,
            offset,
            ..Default::default()
        }
    }

    #[test]
    fn clamp_paging_defaults_and_bounds() {
        assert_eq!(clamp_paging(&filter(None, None)), (DEFAULT_LIST_LIMIT, 0));
        assert_eq!(clamp_paging(&filter(Some(10), Some(20))), (10, 20));
        // 0 / 負値 / 上限超えは安全側へ丸める
        assert_eq!(clamp_paging(&filter(Some(0), Some(-5))), (1, 0));
        assert_eq!(clamp_paging(&filter(Some(-1), None)), (1, 0));
        assert_eq!(
            clamp_paging(&filter(Some(MAX_LIST_LIMIT + 1), None)),
            (MAX_LIST_LIMIT, 0)
        );
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

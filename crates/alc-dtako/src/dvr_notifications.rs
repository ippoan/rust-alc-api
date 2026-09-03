//! theearth の DVR (ドライブレコーダー) 動画通知の受け皿 (Refs ohishi-exp/nuxt-dtako-admin#1094)。
//!
//! dtako-scraper-relay の cron が theearth から動画一覧を取り、
//!
//! 1. `POST /api/dvr/notifications` — 一覧をバッチ ingest して行を起票する。
//!    応答が返す `pending` (新規 + まだ取りに行く価値のある既存行) を relay が回り、
//! 2. `POST /api/dvr/files/{id}` — `.vdf` の生バイトを送る。ここで R2 に保存する。
//!
//! どちらも `internal_shared_secret_router` 配下 (`X-Internal-Shared-Secret` +
//! `X-Tenant-ID`) に merge される。この class は **caller が tenant を名乗る**ので、
//! 行を引くクエリは RLS 任せにせず `tenant_id` を SQL に明示する (二重防御)。
//!
//! 通知 (LINE WORKS) はここでは出さない。宛先の知識は relay 側にあり、
//! relay が既存の送信経路を叩く (#1094 の決定 2)。

use axum::{
    body::Body,
    extract::{Path, State},
    http::StatusCode,
    routing::post,
    Extension, Json, Router,
};
use chrono::{DateTime, Utc};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use alc_core::auth_middleware::TenantId;
use alc_core::tenant::TenantConn;
use alc_core::AppState;

/// 1 リクエストで受けるバッチの上限。
///
/// 実測は約 15 件/日 (8 ヶ月) で、relay の cron は 10 分おきに回る。通常は 1 桁件数
/// しか載らない。上限は「relay 側が長期停止したあとの初回まとめ送り」や再送スパイクで
/// 1 リクエストが無制限に膨らむのを防ぐためのもの。超えたら 400 を返し、relay 側に
/// 分割送信させる (取りこぼしは次の cron で自然に回収される)。
const MAX_BATCH_ITEMS: usize = 200;

/// `serial_no` / `file_name` の長さ上限。どちらも theearth 由来 (untrusted) で、
/// R2 key (`{tenant_id}/dvr/{serial_no}/{file_name}`) の構成要素になる。
const MAX_SERIAL_NO_LEN: usize = 64;
const MAX_FILE_NAME_LEN: usize = 128;

/// 表示用の付随情報 (vehicle_cd / vehicle_name / driver_name / event_type) の長さ上限。
/// キーではないので文字種は縛らないが、長さだけは切る。
const MAX_TEXT_FIELD_LEN: usize = 256;

/// 実体取得を諦めるまでの試行回数。
///
/// これ以上 `attempts` が進んだ行は ingest 応答の `pending` に載せない = relay が
/// もう取りに行かない。theearth 側で動画が消えている / 恒久的に落ちる行を毎 cron
/// (= 1 日 144 回) 引き続けないための足切り。3 回だと 30 分で諦めることになり
/// theearth 側の一時的な不調を拾えないので、約 1 時間ぶんの再試行に相当する 6 にする。
const MAX_FILE_ATTEMPTS: i32 = 6;

/// 受け付ける `.vdf` の最大サイズ (32MB)。Cloud Run の HTTP request body 上限。
///
/// 実測の平均は 375KB なので通常は無関係だが、超過を無言で切り詰めると壊れた動画が
/// `stored` として残るため、明示的に 413 + `file_status='failed'` に落とす。
///
/// このハンドラは body を `Body` (ストリーム) で受けるので、`main.rs` の
/// `DefaultBodyLimit` (バッファリングする抽出器にだけ効く) は適用されない。
/// 上限はここが単独の関門なので、ストリームを読みながら自前で数える。
const MAX_FILE_BYTES: usize = 32 * 1024 * 1024;

/// dtako-scraper-relay から `INTERNAL_SHARED_SECRET` + `X-Tenant-ID` で叩く ingest。
/// `require_internal_shared_secret` middleware 配下に merge される想定。
pub fn internal_router() -> Router<AppState> {
    Router::new()
        .route("/dvr/notifications", post(ingest))
        .route("/dvr/files/{id}", post(upload_file))
}

/// 1 件ぶんの通知 (theearth の一覧 1 行に対応)。
#[derive(Debug, Deserialize)]
pub struct DvrNotificationCreate {
    pub serial_no: String,
    pub file_name: String,
    pub vehicle_cd: Option<String>,
    pub vehicle_name: Option<String>,
    pub driver_name: Option<String>,
    pub event_type: Option<String>,
    /// theearth が示す録画日時 (RFC3339)。欠ける / パースできない場合は省略する。
    pub dvr_datetime: Option<DateTime<Utc>>,
    /// relay が組み立てた動画 URL。記録用で、重複判定には使わない。
    pub source_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DvrIngestBody {
    pub items: Vec<DvrNotificationCreate>,
}

/// relay が次に実体を取りに行くべき 1 行。
#[derive(Debug, Serialize)]
pub struct DvrPendingItem {
    pub id: Uuid,
    pub serial_no: String,
    pub file_name: String,
}

#[derive(Debug, Serialize)]
pub struct DvrIngestResponse {
    /// 今回新しく行が作られた件数。
    pub inserted: i64,
    /// 自然キーが既にあり insert されなかった件数 (`pending` に再掲されたものも含む)。
    pub skipped: i64,
    /// 実体をまだ取りに行くべき行 = 「今回 insert された行」+
    /// 「既存だが `file_status='pending'` かつ `attempts < MAX_FILE_ATTEMPTS` の行」。
    /// 廃止元 (rust-logi) の `RetryPendingDownloads` 相当をこの応答が兼ねる
    /// (#1094 の設計注意 6)。
    pub pending: Vec<DvrPendingItem>,
}

#[derive(Debug, Serialize)]
pub struct DvrFileStoredResponse {
    pub id: Uuid,
    pub file_status: &'static str,
    pub size: i64,
    pub r2_key: String,
}

/// R2 key の構成要素として安全か。theearth 由来の untrusted 値なので、
/// 英数字 + `-` `_` `.` の allowlist に絞り、`..` を弾く (path traversal 防止)。
/// `hub_measurements::valid_session_id` と同型の検証。
fn valid_key_component(v: &str, max_len: usize) -> bool {
    !v.is_empty()
        && v.len() <= max_len
        && !v.contains("..")
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// 表示用フィールドは長さだけ見る (文字種は縛らない — 車番・氏名は日本語が入る)。
fn valid_text_field(v: Option<&String>) -> bool {
    v.map(|s| s.chars().count() <= MAX_TEXT_FIELD_LEN)
        .unwrap_or(true)
}

fn validate(item: &DvrNotificationCreate) -> bool {
    valid_key_component(&item.serial_no, MAX_SERIAL_NO_LEN)
        && valid_key_component(&item.file_name, MAX_FILE_NAME_LEN)
        && valid_text_field(item.vehicle_cd.as_ref())
        && valid_text_field(item.vehicle_name.as_ref())
        && valid_text_field(item.driver_name.as_ref())
        && valid_text_field(item.event_type.as_ref())
        && valid_text_field(item.source_url.as_ref())
}

/// R2 key。既存 dtako の prefix 規約 (`{tenant_id}/uploads/...` /
/// `{tenant_id}/unko/...`) に揃える。
fn r2_key(tenant_id: Uuid, serial_no: &str, file_name: &str) -> String {
    format!("{tenant_id}/dvr/{serial_no}/{file_name}")
}

/// `POST /api/dvr/notifications` — 一覧のバッチ ingest。
///
/// `items` が空なら 400 (relay の bug を無言の 200 で隠さない)。
/// `MAX_BATCH_ITEMS` 超も 400。
async fn ingest(
    State(state): State<AppState>,
    tenant: Extension<TenantId>,
    Json(body): Json<DvrIngestBody>,
) -> Result<Json<DvrIngestResponse>, StatusCode> {
    let items = body.items;
    if items.is_empty() || items.len() > MAX_BATCH_ITEMS {
        return Err(StatusCode::BAD_REQUEST);
    }
    if !items.iter().all(validate) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let tenant_id = tenant.0 .0;
    let mut tc = TenantConn::acquire(state.pool(), &tenant_id.to_string())
        .await
        .map_err(|e| {
            tracing::error!("dvr_notifications acquire error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut inserted: i64 = 0;
    let mut skipped: i64 = 0;
    let mut pending: Vec<DvrPendingItem> = Vec::new();

    for item in &items {
        let new_id: Option<Uuid> = sqlx::query_scalar(
            r#"
            INSERT INTO dvr_notifications (
                tenant_id, serial_no, file_name, vehicle_cd, vehicle_name,
                driver_name, event_type, dvr_datetime, source_url
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (tenant_id, serial_no, file_name) DO NOTHING
            RETURNING id
            "#,
        )
        .bind(tenant_id)
        .bind(&item.serial_no)
        .bind(&item.file_name)
        .bind(&item.vehicle_cd)
        .bind(&item.vehicle_name)
        .bind(&item.driver_name)
        .bind(&item.event_type)
        .bind(item.dvr_datetime)
        .bind(&item.source_url)
        .fetch_optional(&mut *tc.conn)
        .await
        .map_err(|e| {
            tracing::error!("dvr_notifications insert error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        if let Some(id) = new_id {
            inserted += 1;
            pending.push(DvrPendingItem {
                id,
                serial_no: item.serial_no.clone(),
                file_name: item.file_name.clone(),
            });
            continue;
        }

        skipped += 1;
        // 既存行。まだ pending かつ試行回数が上限未満なら、もう一度取りに行かせる。
        let retry: Option<Uuid> = sqlx::query_scalar(
            r#"
            SELECT id
              FROM dvr_notifications
             WHERE tenant_id = $1
               AND serial_no = $2
               AND file_name = $3
               AND file_status = 'pending'
               AND attempts < $4
            "#,
        )
        .bind(tenant_id)
        .bind(&item.serial_no)
        .bind(&item.file_name)
        .bind(MAX_FILE_ATTEMPTS)
        .fetch_optional(&mut *tc.conn)
        .await
        .map_err(|e| {
            tracing::error!("dvr_notifications pending lookup error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        if let Some(id) = retry {
            pending.push(DvrPendingItem {
                id,
                serial_no: item.serial_no.clone(),
                file_name: item.file_name.clone(),
            });
        }
    }

    Ok(Json(DvrIngestResponse {
        inserted,
        skipped,
        pending,
    }))
}

/// 失敗を記録する (`attempts` +1 / `last_error`)。`status` を渡すと `file_status` も更新する。
async fn record_failure(
    conn: &mut sqlx::PgConnection,
    tenant_id: Uuid,
    id: Uuid,
    status: Option<&str>,
    message: &str,
) {
    let sql = match status {
        Some(_) => {
            "UPDATE dvr_notifications
                SET attempts = attempts + 1, last_error = $3, file_status = $4, updated_at = now()
              WHERE id = $1 AND tenant_id = $2"
        }
        None => {
            "UPDATE dvr_notifications
                SET attempts = attempts + 1, last_error = $3, updated_at = now()
              WHERE id = $1 AND tenant_id = $2"
        }
    };
    let mut q = sqlx::query(sql).bind(id).bind(tenant_id).bind(message);
    if let Some(s) = status {
        q = q.bind(s);
    }
    if let Err(e) = q.execute(conn).await {
        tracing::error!("dvr_notifications record_failure error: {e}");
    }
}

/// body を最大 `MAX_FILE_BYTES` まで読む。超えたら `Err(None)`、
/// ストリーム自体が壊れたら `Err(Some(message))` を返す。
async fn read_capped(body: Body) -> Result<Vec<u8>, Option<String>> {
    let mut stream = body.into_data_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| Some(format!("body stream error: {e}")))?;
        if buf.len() + chunk.len() > MAX_FILE_BYTES {
            return Err(None);
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

/// `POST /api/dvr/files/{id}` — `.vdf` の生バイトを受けて R2 に保存する。
///
/// **`{id}` は必ず `X-Tenant-ID` と対で突合する** (`WHERE id = $1 AND tenant_id = $2`)。
/// この router は caller が tenant を名乗る class なので、id だけで行を引くと
/// 別テナントの行を上書きできてしまう。RLS 任せにせず SQL に条件を書く。
/// 「id が無い」と「他人のもの」は区別させず、どちらも 404 にする。
///
/// mp4 には変換せず `.vdf` をそのまま置く (#1094 の決定)。
async fn upload_file(
    State(state): State<AppState>,
    tenant: Extension<TenantId>,
    Path(id): Path<Uuid>,
    body: Body,
) -> Result<Json<DvrFileStoredResponse>, StatusCode> {
    let tenant_id = tenant.0 .0;
    let mut tc = TenantConn::acquire(state.pool(), &tenant_id.to_string())
        .await
        .map_err(|e| {
            tracing::error!("dvr_notifications acquire error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT serial_no, file_name FROM dvr_notifications WHERE id = $1 AND tenant_id = $2",
    )
    .bind(id)
    .bind(tenant_id)
    .fetch_optional(&mut *tc.conn)
    .await
    .map_err(|e| {
        tracing::error!("dvr_notifications lookup error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let (serial_no, file_name) = row.ok_or(StatusCode::NOT_FOUND)?;

    let bytes = match read_capped(body).await {
        Ok(bytes) => bytes,
        Err(None) => {
            record_failure(
                &mut tc.conn,
                tenant_id,
                id,
                Some("failed"),
                &format!("file exceeds {MAX_FILE_BYTES} bytes"),
            )
            .await;
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
        Err(Some(msg)) => {
            record_failure(&mut tc.conn, tenant_id, id, None, &msg).await;
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    let storage = match state.dtako_storage.as_ref() {
        Some(s) => s,
        None => {
            tracing::error!("dvr_notifications: DTAKO_R2_BUCKET not configured");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let key = r2_key(tenant_id, &serial_no, &file_name);
    if let Err(e) = storage
        .upload(&key, &bytes, "application/octet-stream")
        .await
    {
        tracing::error!("dvr_notifications R2 upload error: {e}");
        record_failure(
            &mut tc.conn,
            tenant_id,
            id,
            None,
            &format!("R2 upload failed: {e}"),
        )
        .await;
        return Err(StatusCode::BAD_GATEWAY);
    }

    let size = bytes.len() as i64;
    sqlx::query(
        r#"
        UPDATE dvr_notifications
           SET file_status = 'stored',
               r2_key      = $3,
               size_bytes  = $4,
               last_error  = NULL,
               updated_at  = now()
         WHERE id = $1 AND tenant_id = $2
        "#,
    )
    .bind(id)
    .bind(tenant_id)
    .bind(&key)
    .bind(size)
    .execute(&mut *tc.conn)
    .await
    .map_err(|e| {
        tracing::error!("dvr_notifications stored update error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(DvrFileStoredResponse {
        id,
        file_status: "stored",
        size,
        r2_key: key,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(serial_no: &str, file_name: &str) -> DvrNotificationCreate {
        DvrNotificationCreate {
            serial_no: serial_no.to_string(),
            file_name: file_name.to_string(),
            vehicle_cd: None,
            vehicle_name: None,
            driver_name: None,
            event_type: None,
            dvr_datetime: None,
            source_url: None,
        }
    }

    #[test]
    fn validate_accepts_well_formed_item() {
        assert!(validate(&item("SN-0001", "20260903_012345.vdf")));
        assert!(validate(&item("sn_1", "a.vdf")));
    }

    #[test]
    fn validate_rejects_untrusted_key_components() {
        // 空 / 長すぎ
        assert!(!validate(&item("", "a.vdf")));
        assert!(!validate(&item("sn", "")));
        assert!(!validate(&item(&"x".repeat(65), "a.vdf")));
        assert!(!validate(&item("sn", &"x".repeat(129))));
        // path traversal / セパレータ / 空白
        assert!(!validate(&item("sn", "../../etc/passwd")));
        assert!(!validate(&item("sn", "a/b.vdf")));
        assert!(!validate(&item("sn/..", "a.vdf")));
        assert!(!validate(&item("sn", "a b.vdf")));
        assert!(!validate(&item("sn", "a%00.vdf")));
    }

    #[test]
    fn validate_caps_display_fields() {
        let mut ok = item("sn", "a.vdf");
        ok.vehicle_name = Some("あ".repeat(MAX_TEXT_FIELD_LEN));
        assert!(validate(&ok));

        let mut too_long = item("sn", "a.vdf");
        too_long.driver_name = Some("あ".repeat(MAX_TEXT_FIELD_LEN + 1));
        assert!(!validate(&too_long));
    }

    #[test]
    fn r2_key_follows_dtako_prefix_convention() {
        let tenant = Uuid::nil();
        assert_eq!(
            r2_key(tenant, "SN-1", "a.vdf"),
            format!("{tenant}/dvr/SN-1/a.vdf")
        );
    }

    #[test]
    fn ingest_body_parses_documented_shape() {
        let body: DvrIngestBody = serde_json::from_str(
            r#"{"items":[{"serial_no":"SN-1","file_name":"a.vdf","vehicle_cd":"1234",
                 "vehicle_name":"大宮 100 あ 1234","driver_name":"山田","event_type":"急ブレーキ",
                 "dvr_datetime":"2026-09-03T01:23:45+09:00","source_url":"https://example.test/a"}]}"#,
        )
        .expect("ingest body");
        assert_eq!(body.items.len(), 1);
        assert_eq!(
            body.items[0].dvr_datetime.unwrap().to_rfc3339(),
            "2026-09-02T16:23:45+00:00"
        );
    }

    #[tokio::test]
    async fn read_capped_accepts_small_body_and_rejects_oversize() {
        let small = read_capped(Body::from(vec![7u8; 1024]))
            .await
            .expect("small");
        assert_eq!(small.len(), 1024);

        let over = read_capped(Body::from(vec![0u8; MAX_FILE_BYTES + 1])).await;
        assert!(matches!(over, Err(None)));
    }
}

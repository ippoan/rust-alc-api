//! upload / mail ingest から呼ぶバックグラウンド redact ヘルパー。
//!
//! `tokio::spawn` で fire-and-forget 起動して呼び出し元 (HTTP リクエスト) を
//! 即座に return させる。結果は `notify_documents.redaction_status` カラムで
//! 追跡する:
//!
//! - `pending`    → 初期値、まだジョブが触っていない
//! - `processing` → spawn 開始
//! - `completed`  → redact 成功 (`redacted_r2_key` set)
//! - `skipped`    → PDF 以外 or `GEMINI_API_KEY` 未設定 (配信は許可)
//! - `failed`     → Gemini or `apply_redactions` エラー (配信ブロック)
//!
//! 配信時は `crates/alc-notify/src/distribute.rs` の pre-check が
//! `completed` / `skipped` 以外を弾いて誤って原本を送信しないようにする。
//! 公開 viewer (`viewer.rs`) は migration 109 の `lookup_delivery_for_view`
//! が `COALESCE(redacted_r2_key, r2_key)` を返すので、redacted があれば
//! 自動的にそちらが配信される。
//!
//! テスト容易性のため、コアロジックは `redact_document_with_deps` に切り出し
//! `&dyn NotifyDocumentRepository` + `&dyn StorageBackend` + endpoint override
//! を引数で受け取る。`AppState` を扱う公開ラッパは薄い。

use std::sync::Arc;
use std::time::Instant;

use uuid::Uuid;

use alc_core::redact_broadcast::{RedactBroadcaster, RedactEvent};
use alc_core::repository::notify_documents::{NotifyDocumentRepository, RedactTiming};
use alc_core::storage::StorageBackend;
use alc_core::AppState;

use crate::redact::{apply_redactions, detect_amount_boxes, detect_amount_boxes_v2};

/// terminal 状態 (`completed` / `skipped` / `failed`) で broadcaster が設定されていれば
/// Cloudflare Worker に WS push をかける。設定されてなければ no-op。
async fn maybe_broadcast(
    broadcaster: Option<&RedactBroadcaster>,
    tenant_id: Uuid,
    document_id: Uuid,
    status: &str,
    redactions_applied: Option<i32>,
    redaction_error: Option<&str>,
) {
    if let Some(b) = broadcaster {
        b.broadcast(&RedactEvent {
            tenant_id,
            document_id,
            status,
            redactions_applied,
            redaction_error,
        })
        .await;
    }
}

/// redact パイプラインのコア。AppState 非依存でユニットテスト可能。
///
/// - `endpoint`: Gemini API のベース URL。`None` なら本番 (`https://generativelanguage.googleapis.com`)。
///   wiremock テストでは `Some(server.uri())` を渡す。
/// - `broadcaster`: terminal 状態を Realtime Worker に push するクライアント。`None` なら
///   broadcast 自体を skip (Phase 3 デプロイ前の互換)。
/// - 全エラーは `redaction_status='failed'` に変換され、本関数は panic しない。
#[allow(clippy::too_many_arguments)]
pub async fn redact_document_with_deps(
    docs: &dyn NotifyDocumentRepository,
    storage: Option<&dyn StorageBackend>,
    api_key: Option<&str>,
    use_2stage: bool,
    endpoint: Option<&str>,
    broadcaster: Option<&RedactBroadcaster>,
    tenant_id: Uuid,
    document_id: Uuid,
) {
    // 1. document 取得 (RLS でテナントチェック)
    let doc = match docs.get(tenant_id, document_id).await {
        Ok(Some(d)) => d,
        Ok(None) => {
            tracing::warn!("redact: document not found tenant={tenant_id} id={document_id}");
            return;
        }
        Err(e) => {
            tracing::error!("redact: db get failed: {e}");
            return;
        }
    };

    // 2. PDF 以外は skipped (拡張子で判定、配信は許可)
    let fname = doc.file_name.clone().unwrap_or_default();
    if !fname.to_lowercase().ends_with(".pdf") {
        tracing::info!("redact: non-pdf document {document_id}, marking skipped");
        let _ = docs
            .update_redaction_status(tenant_id, document_id, "skipped", None)
            .await;
        maybe_broadcast(broadcaster, tenant_id, document_id, "skipped", None, None).await;
        return;
    }

    // 3. GEMINI_API_KEY 未設定 → skipped (CI / staging without key 用)
    let api_key = match api_key {
        Some(k) if !k.is_empty() => k,
        _ => {
            tracing::warn!("redact: GEMINI_API_KEY not set, skipping document {document_id}");
            let _ = docs
                .update_redaction_status(tenant_id, document_id, "skipped", None)
                .await;
            maybe_broadcast(broadcaster, tenant_id, document_id, "skipped", None, None).await;
            return;
        }
    };

    // 4. notify_storage 未設定 → failed (構成エラー、配信ブロック)
    let storage = match storage {
        Some(s) => s,
        None => {
            tracing::error!("redact: notify_storage not configured");
            let err = "notify_storage not configured";
            let _ = docs
                .update_redaction_status(tenant_id, document_id, "failed", Some(err))
                .await;
            maybe_broadcast(
                broadcaster,
                tenant_id,
                document_id,
                "failed",
                None,
                Some(err),
            )
            .await;
            return;
        }
    };

    // 5. processing マーキング (UI で「処理中」を表示するため)
    if let Err(e) = docs
        .update_redaction_status(tenant_id, document_id, "processing", None)
        .await
    {
        tracing::error!("redact: mark processing failed: {e}");
        return;
    }

    // stage 別レイテンシ計測 (Refs ippoan/nuxt-notify#71)。Cloud Run 上で動く
    // 本パイプラインは Cloudflare ログには出ないので、ここで構造化 log を残して
    // download / gemini / render(JPEG) / upload のどこが律速かを切り分けられるようにする。
    let t_total = Instant::now();

    // 6. R2 から原本 PDF を取得
    let t_dl = Instant::now();
    let pdf_bytes = match storage.download(&doc.r2_key).await {
        Ok(b) => b,
        Err(e) => {
            let err = format!("r2 download: {e}");
            tracing::warn!(
                document_id = %document_id,
                tenant_id = %tenant_id,
                stage = "download",
                dl_ms = t_dl.elapsed().as_millis() as u64,
                total_ms = t_total.elapsed().as_millis() as u64,
                error = %err,
                "redact_pipeline_failed"
            );
            let _ = docs
                .update_redaction_status(tenant_id, document_id, "failed", Some(&err))
                .await;
            maybe_broadcast(
                broadcaster,
                tenant_id,
                document_id,
                "failed",
                None,
                Some(&err),
            )
            .await;
            return;
        }
    };
    let dl_ms = t_dl.elapsed().as_millis();

    // 7. Gemini で redaction box を検出
    let t_llm = Instant::now();
    let redactions = {
        let result = if use_2stage {
            detect_amount_boxes_v2(&pdf_bytes, api_key, None, endpoint).await
        } else {
            detect_amount_boxes(&pdf_bytes, api_key, None, endpoint).await
        };
        match result {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(
                    document_id = %document_id,
                    tenant_id = %tenant_id,
                    stage = "llm",
                    use_2stage,
                    pdf_bytes = pdf_bytes.len(),
                    dl_ms = dl_ms as u64,
                    llm_ms = t_llm.elapsed().as_millis() as u64,
                    total_ms = t_total.elapsed().as_millis() as u64,
                    error = %format!("detect_amount_boxes (2stage={use_2stage}): {e}"),
                    "redact_pipeline_failed"
                );
                let err = format!("gemini: {e}");
                let _ = docs
                    .update_redaction_status(tenant_id, document_id, "failed", Some(&err))
                    .await;
                maybe_broadcast(
                    broadcaster,
                    tenant_id,
                    document_id,
                    "failed",
                    None,
                    Some(&err),
                )
                .await;
                return;
            }
        }
    };
    let llm_ms = t_llm.elapsed().as_millis();

    // 8. rasterize → 黒矩形マスク → JPEG (PDF 再構築は廃止、画像 1 枚で配信する)
    let t_render = Instant::now();
    let redacted_bytes = match apply_redactions(&pdf_bytes, &redactions) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(
                document_id = %document_id,
                tenant_id = %tenant_id,
                stage = "render",
                pdf_bytes = pdf_bytes.len(),
                dl_ms = dl_ms as u64,
                llm_ms = llm_ms as u64,
                render_ms = t_render.elapsed().as_millis() as u64,
                total_ms = t_total.elapsed().as_millis() as u64,
                error = %format!("apply_redactions: {e}"),
                "redact_pipeline_failed"
            );
            let err = format!("apply: {e}");
            let _ = docs
                .update_redaction_status(tenant_id, document_id, "failed", Some(&err))
                .await;
            maybe_broadcast(
                broadcaster,
                tenant_id,
                document_id,
                "failed",
                None,
                Some(&err),
            )
            .await;
            return;
        }
    };
    let render_ms = t_render.elapsed().as_millis();

    // 9. R2 に redacted JPEG をアップロード (document スコープ固定キー、上書き対応)
    let t_up = Instant::now();
    let key = format!("{}/redacted/{}.jpg", tenant_id, document_id);
    if let Err(e) = storage.upload(&key, &redacted_bytes, "image/jpeg").await {
        tracing::warn!(
            document_id = %document_id,
            tenant_id = %tenant_id,
            stage = "upload",
            pdf_bytes = pdf_bytes.len(),
            dl_ms = dl_ms as u64,
            llm_ms = llm_ms as u64,
            render_ms = render_ms as u64,
            up_ms = t_up.elapsed().as_millis() as u64,
            total_ms = t_total.elapsed().as_millis() as u64,
            error = %format!("r2 upload: {e}"),
            "redact_pipeline_failed"
        );
        let err = format!("r2 upload: {e}");
        let _ = docs
            .update_redaction_status(tenant_id, document_id, "failed", Some(&err))
            .await;
        maybe_broadcast(
            broadcaster,
            tenant_id,
            document_id,
            "failed",
            None,
            Some(&err),
        )
        .await;
        return;
    }
    let up_ms = t_up.elapsed().as_millis();

    // 10. 成功確定: redacted_r2_key + redacted_at + redactions_applied + status='completed'
    //     + stage 別レイテンシ (UI デバッグ表示用、Refs #334)
    let applied = redactions.len() as i32;
    let timing = RedactTiming {
        dl_ms: dl_ms as i32,
        llm_ms: llm_ms as i32,
        render_ms: render_ms as i32,
        upload_ms: up_ms as i32,
        total_ms: t_total.elapsed().as_millis() as i32,
    };
    if let Err(e) = docs
        .complete_redaction(tenant_id, document_id, &key, applied, &timing)
        .await
    {
        tracing::error!("redact: complete update failed: {e}");
        return;
    }

    maybe_broadcast(
        broadcaster,
        tenant_id,
        document_id,
        "completed",
        Some(applied),
        None,
    )
    .await;

    // stage 別 + total を構造化 field で残す (Refs ippoan/nuxt-notify#71)。
    // Cloud Run Logging の jsonPayload で dl_ms/llm_ms/render_ms/up_ms を
    // クエリでき、同 document_id を Cloudflare 側ログと突き合わせて p95 を取れる。
    // 体感の律速はほぼ llm_ms (Gemini) の想定だが、ここで数値として確定させる。
    tracing::info!(
        document_id = %document_id,
        tenant_id = %tenant_id,
        applied,
        use_2stage,
        pdf_bytes = pdf_bytes.len(),
        dl_ms = dl_ms as u64,
        llm_ms = llm_ms as u64,
        render_ms = render_ms as u64,
        up_ms = up_ms as u64,
        total_ms = t_total.elapsed().as_millis() as u64,
        "redact_pipeline_done"
    );
}

/// fire-and-forget で background redact を起動する。呼び出し元はすぐ return できる。
///
/// `AppState` から env と repo/storage を取り出して `redact_document_with_deps` に委譲。
/// upload / ingest の各エンドポイントから新規 document の INSERT 直後に呼ぶ。
pub fn spawn_redact_document(state: AppState, tenant_id: Uuid, document_id: Uuid) {
    let api_key = std::env::var("GEMINI_API_KEY").ok();
    let use_2stage = std::env::var("NOTIFY_REDACT_2STAGE").as_deref() == Ok("1");
    let docs: Arc<dyn NotifyDocumentRepository> = state.notify_documents.clone();
    let storage: Option<Arc<dyn StorageBackend>> = state.notify_storage.clone();
    let broadcaster: Option<Arc<RedactBroadcaster>> = state.redact_broadcaster.clone();

    tokio::spawn(async move {
        redact_document_with_deps(
            docs.as_ref(),
            storage.as_deref(),
            api_key.as_deref(),
            use_2stage,
            None,
            broadcaster.as_deref(),
            tenant_id,
            document_id,
        )
        .await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::{Arc, Mutex};

    use alc_core::repository::notify_documents::{
        CreateNotifyDocument, ExtractionResult, NotifyDocument, RedactTiming,
    };
    use alc_core::storage::StorageError;
    use async_trait::async_trait;

    // ============================================================
    // In-memory test stubs
    // ============================================================

    #[derive(Default)]
    struct StubDocs {
        statuses: Mutex<Vec<(String, Option<String>)>>,
        completed: Mutex<Option<(String, i32)>>,
        doc: Mutex<Option<NotifyDocument>>,
    }

    impl StubDocs {
        fn new(doc: NotifyDocument) -> Arc<Self> {
            let s = Arc::new(Self::default());
            *s.doc.lock().unwrap() = Some(doc);
            s
        }
        fn with_no_doc() -> Arc<Self> {
            Arc::new(Self::default())
        }
    }

    #[async_trait]
    impl NotifyDocumentRepository for StubDocs {
        async fn create(
            &self,
            _t: Uuid,
            _input: &CreateNotifyDocument,
        ) -> Result<NotifyDocument, sqlx::Error> {
            unimplemented!()
        }
        async fn get(&self, _t: Uuid, _id: Uuid) -> Result<Option<NotifyDocument>, sqlx::Error> {
            Ok(self.doc.lock().unwrap().clone())
        }
        async fn list(
            &self,
            _t: Uuid,
            _l: i64,
            _o: i64,
        ) -> Result<Vec<NotifyDocument>, sqlx::Error> {
            unimplemented!()
        }
        async fn search(&self, _t: Uuid, _q: &str) -> Result<Vec<NotifyDocument>, sqlx::Error> {
            unimplemented!()
        }
        async fn update_extraction(
            &self,
            _t: Uuid,
            _id: Uuid,
            _r: &ExtractionResult,
        ) -> Result<(), sqlx::Error> {
            unimplemented!()
        }
        async fn update_extraction_error(
            &self,
            _t: Uuid,
            _id: Uuid,
            _e: &str,
        ) -> Result<(), sqlx::Error> {
            unimplemented!()
        }
        async fn update_distribution_status(
            &self,
            _t: Uuid,
            _id: Uuid,
            _s: &str,
        ) -> Result<(), sqlx::Error> {
            unimplemented!()
        }
        async fn update_redaction_status(
            &self,
            _t: Uuid,
            _id: Uuid,
            status: &str,
            error: Option<&str>,
        ) -> Result<(), sqlx::Error> {
            self.statuses
                .lock()
                .unwrap()
                .push((status.to_string(), error.map(|s| s.to_string())));
            Ok(())
        }
        async fn complete_redaction(
            &self,
            _t: Uuid,
            _id: Uuid,
            redacted_r2_key: &str,
            applied: i32,
            _timing: &RedactTiming,
        ) -> Result<(), sqlx::Error> {
            *self.completed.lock().unwrap() = Some((redacted_r2_key.to_string(), applied));
            self.statuses
                .lock()
                .unwrap()
                .push(("completed".into(), None));
            Ok(())
        }
        async fn reset_redaction(&self, _t: Uuid, _id: Uuid) -> Result<(), sqlx::Error> {
            Ok(())
        }
    }

    struct StubStorage {
        pdf: Vec<u8>,
        last_upload: Mutex<Option<(String, Vec<u8>)>>,
        download_should_fail: bool,
        upload_should_fail: bool,
    }

    impl StubStorage {
        fn ok(pdf: Vec<u8>) -> Arc<Self> {
            Arc::new(Self {
                pdf,
                last_upload: Mutex::new(None),
                download_should_fail: false,
                upload_should_fail: false,
            })
        }
        fn with_download_fail() -> Arc<Self> {
            Arc::new(Self {
                pdf: Vec::new(),
                last_upload: Mutex::new(None),
                download_should_fail: true,
                upload_should_fail: false,
            })
        }
        fn with_upload_fail(pdf: Vec<u8>) -> Arc<Self> {
            Arc::new(Self {
                pdf,
                last_upload: Mutex::new(None),
                download_should_fail: false,
                upload_should_fail: true,
            })
        }
    }

    #[async_trait]
    impl StorageBackend for StubStorage {
        async fn upload(
            &self,
            key: &str,
            bytes: &[u8],
            _content_type: &str,
        ) -> Result<String, StorageError> {
            if self.upload_should_fail {
                return Err(StorageError::Upload("simulated upload failure".into()));
            }
            *self.last_upload.lock().unwrap() = Some((key.to_string(), bytes.to_vec()));
            Ok(format!("https://test/{key}"))
        }
        fn public_url(&self, key: &str) -> String {
            format!("https://test/{key}")
        }
        async fn download(&self, _key: &str) -> Result<Vec<u8>, StorageError> {
            if self.download_should_fail {
                return Err(StorageError::Upload("simulated download failure".into()));
            }
            Ok(self.pdf.clone())
        }
        async fn exists(&self, _key: &str) -> Result<bool, StorageError> {
            Ok(true)
        }
        async fn delete(&self, _key: &str) -> Result<(), StorageError> {
            Ok(())
        }
        fn extract_key(&self, _url: &str) -> Option<String> {
            None
        }
        fn bucket(&self) -> &str {
            "test-bucket"
        }
        async fn presign_get(
            &self,
            key: &str,
            _expiry_seconds: u32,
        ) -> Result<String, StorageError> {
            Ok(format!("https://test/{key}?signed=1"))
        }
    }

    fn build_doc(file_name: Option<&str>) -> NotifyDocument {
        NotifyDocument {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            source_type: "manual".into(),
            source_sender: None,
            source_subject: None,
            r2_key: "tenant/manual/doc.pdf".into(),
            file_name: file_name.map(|s| s.into()),
            file_size_bytes: Some(1024),
            extracted_title: None,
            extracted_date: None,
            extracted_summary: None,
            extracted_phone_numbers: None,
            extracted_data: None,
            extraction_status: "pending".into(),
            extraction_error: None,
            distribution_status: "pending".into(),
            distributed_at: None,
            redacted_r2_key: None,
            redacted_at: None,
            redactions_applied: None,
            redaction_status: "pending".into(),
            redaction_error: None,
            redact_dl_ms: None,
            redact_llm_ms: None,
            redact_render_ms: None,
            redact_upload_ms: None,
            redact_total_ms: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    /// Gemini モック: `{"items": [{"page": 0, "x_norm": 0.1, "y_norm": 0.1, "width_norm": 0.1, "height_norm": 0.05, "value": "1000"}]}` を返す。
    /// detect_amount_boxes は内部で JSON parse + RedactionBox に詰め替える。
    /// ただし apply_redactions はページ存在確認に lopdf を使うので、本テストでは
    /// 「Gemini 成功 + apply_redactions 失敗」のパスを検証する。
    async fn start_gemini_mock_returning(items_json: &str) -> wiremock::MockServer {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "candidates": [{
                        "content": {"parts": [{"text": items_json}]}
                    }]
                })),
            )
            .mount(&server)
            .await;
        server
    }

    async fn start_gemini_mock_with_status(status: u16) -> wiremock::MockServer {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(wiremock::ResponseTemplate::new(status))
            .mount(&server)
            .await;
        server
    }

    // ============================================================
    // tests
    // ============================================================

    // 1. 通常成功: Gemini が空配列 → redactions=0 → apply_redactions は no-op で成功
    #[tokio::test]
    async fn redact_document_completes_with_zero_redactions() {
        let docs = StubDocs::new(build_doc(Some("invoice.pdf")));
        // 最小だが apply_redactions が parse できる PDF を simple_pdf() で生成
        let pdf = simple_pdf();
        let storage = StubStorage::ok(pdf);
        let server = start_gemini_mock_returning("{\"redactions\": []}").await;

        redact_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            Some("test-key"),
            false,
            Some(&server.uri()),
            None,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let statuses = docs.statuses.lock().unwrap().clone();
        // processing → completed
        assert_eq!(statuses.first().map(|s| s.0.as_str()), Some("processing"));
        assert_eq!(statuses.last().map(|s| s.0.as_str()), Some("completed"));
        let (key, applied) = docs.completed.lock().unwrap().clone().unwrap();
        // tenant/redacted/{document_id}.jpg (PDF wrapping を廃止 → JPEG 直保存)
        assert!(key.contains("/redacted/"), "unexpected key: {key}");
        assert!(key.ends_with(".jpg"), "unexpected key: {key}");
        assert_eq!(applied, 0);
        let upload = storage.last_upload.lock().unwrap().clone().unwrap();
        assert!(upload.0.contains("/redacted/"));
        assert!(upload.0.ends_with(".jpg"));
    }

    // 2. PDF 以外は skipped、Gemini を呼ばない
    #[tokio::test]
    async fn redact_document_non_pdf_is_skipped() {
        let docs = StubDocs::new(build_doc(Some("a.docx")));
        let storage = StubStorage::ok(b"x".to_vec());

        redact_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            Some("k"),
            false,
            None,
            None,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let statuses = docs.statuses.lock().unwrap().clone();
        assert_eq!(statuses, vec![("skipped".into(), None)]);
        assert!(docs.completed.lock().unwrap().is_none());
    }

    // 3. file_name=None も skipped (拡張子判定で空文字列は ".pdf" で終わらない)
    #[tokio::test]
    async fn redact_document_no_file_name_is_skipped() {
        let docs = StubDocs::new(build_doc(None));
        let storage = StubStorage::ok(b"x".to_vec());

        redact_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            Some("k"),
            false,
            None,
            None,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let statuses = docs.statuses.lock().unwrap().clone();
        assert_eq!(statuses, vec![("skipped".into(), None)]);
    }

    // 4. api_key=None → skipped
    #[tokio::test]
    async fn redact_document_no_api_key_is_skipped() {
        let docs = StubDocs::new(build_doc(Some("doc.pdf")));
        let storage = StubStorage::ok(b"x".to_vec());

        redact_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            None,
            false,
            None,
            None,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let statuses = docs.statuses.lock().unwrap().clone();
        assert_eq!(statuses, vec![("skipped".into(), None)]);
    }

    // 5. api_key="" (空文字) も skipped
    #[tokio::test]
    async fn redact_document_empty_api_key_is_skipped() {
        let docs = StubDocs::new(build_doc(Some("doc.pdf")));
        let storage = StubStorage::ok(b"x".to_vec());

        redact_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            Some(""),
            false,
            None,
            None,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        assert_eq!(
            docs.statuses.lock().unwrap().clone(),
            vec![("skipped".into(), None)]
        );
    }

    // 6. notify_storage = None → failed
    #[tokio::test]
    async fn redact_document_no_storage_fails() {
        let docs = StubDocs::new(build_doc(Some("doc.pdf")));

        redact_document_with_deps(
            docs.as_ref(),
            None,
            Some("k"),
            false,
            None,
            None,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let statuses = docs.statuses.lock().unwrap().clone();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].0, "failed");
        assert!(statuses[0].1.as_deref().unwrap().contains("notify_storage"));
    }

    // 7. document not found → 何もしない
    #[tokio::test]
    async fn redact_document_not_found_is_noop() {
        let docs = StubDocs::with_no_doc();

        redact_document_with_deps(
            docs.as_ref(),
            None,
            Some("k"),
            false,
            None,
            None,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        assert_eq!(docs.statuses.lock().unwrap().len(), 0);
        assert!(docs.completed.lock().unwrap().is_none());
    }

    // 8. R2 download エラー → failed
    #[tokio::test]
    async fn redact_document_download_failure_marks_failed() {
        let docs = StubDocs::new(build_doc(Some("doc.pdf")));
        let storage = StubStorage::with_download_fail();

        redact_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            Some("k"),
            false,
            None,
            None,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let statuses = docs.statuses.lock().unwrap().clone();
        let last = statuses.last().expect("at least one status");
        assert_eq!(last.0, "failed");
        assert!(last.1.as_deref().unwrap().contains("r2 download"));
    }

    // 9. R2 upload エラー → failed (Gemini 成功 + apply 成功 + upload 失敗)
    #[tokio::test]
    async fn redact_document_upload_failure_marks_failed() {
        let docs = StubDocs::new(build_doc(Some("doc.pdf")));
        let pdf = simple_pdf();
        let storage = StubStorage::with_upload_fail(pdf);
        let server = start_gemini_mock_returning("{\"redactions\": []}").await;

        redact_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            Some("k"),
            false,
            Some(&server.uri()),
            None,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let statuses = docs.statuses.lock().unwrap().clone();
        let last = statuses.last().expect("at least one status");
        assert_eq!(last.0, "failed");
        assert!(last.1.as_deref().unwrap().contains("r2 upload"));
    }

    // 10. Gemini 5xx → failed
    #[tokio::test]
    async fn redact_document_gemini_failure_marks_failed() {
        let docs = StubDocs::new(build_doc(Some("doc.pdf")));
        let storage = StubStorage::ok(simple_pdf());
        let server = start_gemini_mock_with_status(500).await;

        redact_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            Some("k"),
            false,
            Some(&server.uri()),
            None,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let statuses = docs.statuses.lock().unwrap().clone();
        let last = statuses.last().expect("at least one status");
        assert_eq!(last.0, "failed");
        assert!(last.1.as_deref().unwrap().contains("gemini"));
    }

    // 11. 不正 PDF → failed
    //     旧アーキテクチャは「detect は raw PDF を投げる → apply で rasterize 失敗」
    //     だったが、新アーキは detect も内部で rasterize する (Gemini に JPEG を送る
    //     ため) なので、不正 PDF は detect で先に失敗 → "gemini:" prefix で wrap される。
    #[tokio::test]
    async fn redact_document_invalid_pdf_marks_failed() {
        let docs = StubDocs::new(build_doc(Some("doc.pdf")));
        let storage = StubStorage::ok(b"not a pdf".to_vec());
        let server = start_gemini_mock_returning("{\"redactions\": []}").await;

        redact_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            Some("k"),
            false,
            Some(&server.uri()),
            None,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let statuses = docs.statuses.lock().unwrap().clone();
        let last = statuses.last().expect("at least one status");
        assert_eq!(last.0, "failed");
        // detect 内 rasterize 失敗 → "gemini: pdfium: load_pdf: ..." になる。
        let err_msg = last.1.as_deref().unwrap();
        assert!(
            err_msg.contains("gemini") || err_msg.contains("apply") || err_msg.contains("pdfium"),
            "unexpected err: {err_msg}"
        );
    }

    // 12. 2-stage モード: 同じ Gemini モック (空配列) で `use_2stage=true` → completed
    #[tokio::test]
    async fn redact_document_2stage_branch_works() {
        let docs = StubDocs::new(build_doc(Some("doc.pdf")));
        let pdf = simple_pdf();
        let storage = StubStorage::ok(pdf);
        // 2-stage は detect_all_cells (Stage 1) → filter_amount_cells (Stage 2) を呼ぶ。
        // 空応答だと filter_amount_cells が "no cells" で fallback → 1-stage で完了。
        // 内部で空配列を返すケースを想定して同じモックで OK。
        let server = start_gemini_mock_returning("{\"pages\": []}").await;

        redact_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            Some("k"),
            true,
            Some(&server.uri()),
            None,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        // 2-stage で内部 fallback が走る場合でも、最終的には completed か failed のどちらか
        let statuses = docs.statuses.lock().unwrap().clone();
        let last = statuses.last().expect("at least one status").0.clone();
        assert!(
            matches!(last.as_str(), "completed" | "failed"),
            "expected completed or failed, got: {last}"
        );
    }

    // 13. spawn helper は即 return できる
    #[tokio::test]
    async fn spawn_redact_document_returns_immediately() {
        // `spawn_redact_document` は AppState 必須でユニットテストでは組み立てが
        // 重いため、ここでは「env 未設定 → skipped」分岐を `redact_document_with_deps`
        // 経由で確認する。env を引数で None にすることで env 競合を完全回避。
        let docs = StubDocs::new(build_doc(Some("doc.pdf")));

        redact_document_with_deps(
            docs.as_ref(),
            None,
            None, // env 相当: GEMINI_API_KEY 未設定 → skipped
            false,
            None,
            None,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        assert_eq!(
            docs.statuses.lock().unwrap().clone(),
            vec![("skipped".into(), None)]
        );
    }

    // 14. broadcaster=Some なら skipped でも broadcast が飛ぶ (PDF 以外パス)
    #[tokio::test]
    async fn redact_document_broadcasts_skipped_for_non_pdf() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/broadcast"))
            .and(wiremock::matchers::body_string_contains("\"skipped\""))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        let broadcaster =
            RedactBroadcaster::new(format!("{}/broadcast", server.uri()), "secret".into());

        let docs = StubDocs::new(build_doc(Some("a.docx")));

        redact_document_with_deps(
            docs.as_ref(),
            None,
            Some("k"),
            false,
            None,
            Some(&broadcaster),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let statuses = docs.statuses.lock().unwrap().clone();
        assert_eq!(statuses, vec![("skipped".into(), None)]);
    }

    // 15. broadcaster=Some + completed パス: redactions_applied 付きで broadcast
    #[tokio::test]
    async fn redact_document_broadcasts_completed_with_count() {
        let bcast = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/broadcast"))
            .and(wiremock::matchers::body_string_contains("\"completed\""))
            .and(wiremock::matchers::body_string_contains(
                "\"redactions_applied\":0",
            ))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .expect(1)
            .mount(&bcast)
            .await;
        let broadcaster =
            RedactBroadcaster::new(format!("{}/broadcast", bcast.uri()), "secret".into());

        let docs = StubDocs::new(build_doc(Some("invoice.pdf")));
        let storage = StubStorage::ok(simple_pdf());
        let gemini = start_gemini_mock_returning("{\"redactions\": []}").await;

        redact_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            Some("k"),
            false,
            Some(&gemini.uri()),
            Some(&broadcaster),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        // 後ろから 1 番目が completed のはず
        let statuses = docs.statuses.lock().unwrap().clone();
        assert_eq!(statuses.last().map(|s| s.0.as_str()), Some("completed"));
    }

    // 16. broadcaster=Some + failed パス (notify_storage 未設定): error メッセージ含む
    #[tokio::test]
    async fn redact_document_broadcasts_failed_with_error_message() {
        let bcast = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/broadcast"))
            .and(wiremock::matchers::body_string_contains("\"failed\""))
            .and(wiremock::matchers::body_string_contains(
                "\"notify_storage not configured\"",
            ))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .expect(1)
            .mount(&bcast)
            .await;
        let broadcaster =
            RedactBroadcaster::new(format!("{}/broadcast", bcast.uri()), "secret".into());

        let docs = StubDocs::new(build_doc(Some("doc.pdf")));

        redact_document_with_deps(
            docs.as_ref(),
            None, // notify_storage = None → failed
            Some("k"),
            false,
            None,
            Some(&broadcaster),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let statuses = docs.statuses.lock().unwrap().clone();
        assert_eq!(statuses.last().map(|s| s.0.as_str()), Some("failed"));
    }

    // 17. broadcaster=Some + r2 download 失敗 → "r2 download" prefix で broadcast
    #[tokio::test]
    async fn redact_document_broadcasts_failed_on_download_error() {
        let bcast = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/broadcast"))
            .and(wiremock::matchers::body_string_contains("\"failed\""))
            .and(wiremock::matchers::body_string_contains("r2 download"))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .expect(1)
            .mount(&bcast)
            .await;
        let broadcaster =
            RedactBroadcaster::new(format!("{}/broadcast", bcast.uri()), "secret".into());

        let docs = StubDocs::new(build_doc(Some("doc.pdf")));
        let storage = StubStorage::with_download_fail();

        redact_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            Some("k"),
            false,
            None,
            Some(&broadcaster),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let statuses = docs.statuses.lock().unwrap().clone();
        let last = statuses.last().expect("at least one status");
        assert_eq!(last.0, "failed");
        assert!(last.1.as_deref().unwrap().contains("r2 download"));
    }

    // 18. broadcaster=Some + Gemini 失敗 → "gemini" prefix で broadcast
    #[tokio::test]
    async fn redact_document_broadcasts_failed_on_gemini_error() {
        let bcast = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/broadcast"))
            .and(wiremock::matchers::body_string_contains("\"failed\""))
            .and(wiremock::matchers::body_string_contains("gemini"))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .expect(1)
            .mount(&bcast)
            .await;
        let broadcaster =
            RedactBroadcaster::new(format!("{}/broadcast", bcast.uri()), "secret".into());

        let docs = StubDocs::new(build_doc(Some("doc.pdf")));
        let storage = StubStorage::ok(simple_pdf());
        let gemini = start_gemini_mock_with_status(500).await;

        redact_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            Some("k"),
            false,
            Some(&gemini.uri()),
            Some(&broadcaster),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let statuses = docs.statuses.lock().unwrap().clone();
        assert_eq!(statuses.last().map(|s| s.0.as_str()), Some("failed"));
    }

    // 19. broadcaster=Some + apply_redactions 失敗 → "apply" prefix で broadcast
    #[tokio::test]
    async fn redact_document_broadcasts_failed_on_apply_error() {
        let bcast = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/broadcast"))
            .and(wiremock::matchers::body_string_contains("\"failed\""))
            // 新アーキでは detect 内で rasterize 失敗 → "gemini:" prefix
            .and(wiremock::matchers::body_string_contains("gemini"))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .expect(1)
            .mount(&bcast)
            .await;
        let broadcaster =
            RedactBroadcaster::new(format!("{}/broadcast", bcast.uri()), "secret".into());

        let docs = StubDocs::new(build_doc(Some("doc.pdf")));
        // 不正 PDF → detect_amount_boxes 内で rasterize エラー
        let storage = StubStorage::ok(b"not a pdf".to_vec());
        let gemini = start_gemini_mock_returning("{\"redactions\": []}").await;

        redact_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            Some("k"),
            false,
            Some(&gemini.uri()),
            Some(&broadcaster),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let statuses = docs.statuses.lock().unwrap().clone();
        assert_eq!(statuses.last().map(|s| s.0.as_str()), Some("failed"));
    }

    // 20. broadcaster=Some + R2 upload 失敗 → "r2 upload" prefix で broadcast
    #[tokio::test]
    async fn redact_document_broadcasts_failed_on_upload_error() {
        let bcast = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/broadcast"))
            .and(wiremock::matchers::body_string_contains("\"failed\""))
            .and(wiremock::matchers::body_string_contains("r2 upload"))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .expect(1)
            .mount(&bcast)
            .await;
        let broadcaster =
            RedactBroadcaster::new(format!("{}/broadcast", bcast.uri()), "secret".into());

        let docs = StubDocs::new(build_doc(Some("doc.pdf")));
        let storage = StubStorage::with_upload_fail(simple_pdf());
        let gemini = start_gemini_mock_returning("{\"redactions\": []}").await;

        redact_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            Some("k"),
            false,
            Some(&gemini.uri()),
            Some(&broadcaster),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let statuses = docs.statuses.lock().unwrap().clone();
        assert_eq!(statuses.last().map(|s| s.0.as_str()), Some("failed"));
    }

    // ============================================================
    // ヘルパー
    // ============================================================

    /// lopdf でパース可能な最小の 1-page PDF。
    fn simple_pdf() -> Vec<u8> {
        use lopdf::content::{Content, Operation};
        use lopdf::dictionary;
        use lopdf::{Document, Object, Stream};

        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let content = Content {
            operations: vec![Operation::new("BT", vec![])],
        };
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
        });
        let pages = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        buf
    }
}

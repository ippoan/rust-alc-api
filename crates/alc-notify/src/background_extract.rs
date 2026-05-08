//! upload / mail ingest から呼ぶバックグラウンド「物流情報抽出」ヘルパー。
//!
//! `tokio::spawn` で fire-and-forget 起動して呼び出し元 (HTTP リクエスト) を
//! 即座に return させる。結果は `notify_documents.extraction_status` および
//! `notify_documents.extracted_data->'logistics'` で追跡する:
//!
//! - `pending`   → 初期値 (まだジョブが触っていない)
//! - `completed` → `update_extraction` 呼び出し成功 (5 フィールド全 null も含む)
//! - `failed`    → Gemini や R2 取得でエラー (`extraction_error` にメッセージ)
//!
//! 配信本文は `distribute::build_distribute_message` が `extracted_data->'logistics'`
//! の有無を見て物流テンプレ / 既存テンプレを切り替える。
//!
//! redact パイプライン (`background_redaction.rs`) と同じ並走前提で書いている:
//!   - redact は `redaction_status` カラム、extract は `extraction_status` カラム
//!     と独立 (migration 070 と migration 109)。書き込みは衝突しない
//!   - 両者とも `state.notify_documents` `state.notify_storage` を共有して読む
//!   - `tokio::spawn` で並列 → Gemini API は 2 リクエスト同時 (rate limit 内)
//!
//! テスト容易性のため、コアロジックは `extract_document_with_deps` に切り出し
//! `&dyn NotifyDocumentRepository` + `&dyn StorageBackend` + endpoint override
//! を引数で受け取る。`AppState` を扱う公開ラッパは薄い。

use std::sync::Arc;

use uuid::Uuid;

use alc_core::repository::notify_documents::{ExtractionResult, NotifyDocumentRepository};
use alc_core::storage::StorageBackend;
use alc_core::AppState;

use crate::extract::{extract_logistics_fields_with_endpoint, ExtractError, LogisticsFields};

/// extract パイプラインのコア。AppState 非依存でユニットテスト可能。
///
/// - `endpoint` / `model`: Gemini API のベース URL とモデル名。`None` なら本番デフォルト。
///   wiremock テストでは `Some(server.uri())` を渡す。
/// - 全エラーは `extraction_status='failed'` に変換され、本関数は panic しない。
/// - 成功 (Gemini が全 null を返した場合も含む) は `extraction_status='completed'`、
///   非空のフィールドだけ `extracted_data.logistics` に格納される。
#[allow(clippy::too_many_arguments)]
pub async fn extract_document_with_deps(
    docs: &dyn NotifyDocumentRepository,
    storage: Option<&dyn StorageBackend>,
    api_key: Option<&str>,
    endpoint: Option<&str>,
    model: Option<&str>,
    tenant_id: Uuid,
    document_id: Uuid,
) {
    // 1. document 取得 (RLS でテナントチェック)
    let doc = match docs.get(tenant_id, document_id).await {
        Ok(Some(d)) => d,
        Ok(None) => {
            tracing::warn!("extract: document not found tenant={tenant_id} id={document_id}");
            return;
        }
        Err(e) => {
            tracing::error!("extract: db get failed: {e}");
            return;
        }
    };

    // 2. PDF 以外は「抽出対象なし」で完了扱い (status=completed, logistics なし)
    let fname = doc.file_name.clone().unwrap_or_default();
    if !fname.to_lowercase().ends_with(".pdf") {
        tracing::info!("extract: non-pdf document {document_id}, marking completed (no logistics)");
        write_no_logistics(docs, tenant_id, document_id, &doc).await;
        return;
    }

    // 3. GEMINI_API_KEY 未設定 → 同様に完了扱い (config issue は per-doc には残さない)
    let api_key = match api_key {
        Some(k) if !k.is_empty() => k,
        _ => {
            tracing::warn!(
                "extract: GEMINI_API_KEY not set, marking completed without logistics for {document_id}"
            );
            write_no_logistics(docs, tenant_id, document_id, &doc).await;
            return;
        }
    };

    // 4. notify_storage 未設定 → 構成エラー → failed
    let storage = match storage {
        Some(s) => s,
        None => {
            tracing::error!("extract: notify_storage not configured");
            let _ = docs
                .update_extraction_error(tenant_id, document_id, "notify_storage not configured")
                .await;
            return;
        }
    };

    // 5. R2 から PDF 取得
    let pdf_bytes = match storage.download(&doc.r2_key).await {
        Ok(b) => b,
        Err(e) => {
            let err = format!("r2 download: {e}");
            let _ = docs
                .update_extraction_error(tenant_id, document_id, &err)
                .await;
            return;
        }
    };

    // 6. Gemini で 5 フィールド抽出
    let endpoint = endpoint.unwrap_or("https://generativelanguage.googleapis.com/v1beta");
    let model = model.unwrap_or("gemini-3.1-flash-lite-preview");

    let fields: LogisticsFields =
        match extract_logistics_fields_with_endpoint(endpoint, model, &pdf_bytes, api_key).await {
            Ok(f) => f,
            Err(e) => {
                let err = match &e {
                    ExtractError::GeminiHttp(_)
                    | ExtractError::GeminiStatus(_, _)
                    | ExtractError::GeminiEmpty
                    | ExtractError::GeminiParse(_) => format!("gemini: {e}"),
                    ExtractError::LogisticsParse(_) => format!("parse: {e}"),
                };
                let _ = docs
                    .update_extraction_error(tenant_id, document_id, &err)
                    .await;
                return;
            }
        };

    // 7. extracted_data の `logistics` キーを書き換え (他キーは保持)
    let merged_data = merge_logistics_into_data(doc.extracted_data.clone(), &fields);

    // 8. 既存 extracted_* フィールドは保持して update_extraction
    let result = ExtractionResult {
        title: doc.extracted_title.clone(),
        date: doc.extracted_date,
        summary: doc.extracted_summary.clone(),
        phone_numbers: doc.extracted_phone_numbers.clone().unwrap_or_default(),
        data: merged_data,
    };
    if let Err(e) = docs
        .update_extraction(tenant_id, document_id, &result)
        .await
    {
        tracing::error!("extract: update_extraction failed: {e}");
        return;
    }

    tracing::info!(
        "extract: tenant={tenant_id} doc={document_id} has_logistics={} (fields: lp={} ulp={} la={} ula={} n={})",
        fields.has_any(),
        fields.loading_place.is_some(),
        fields.unloading_place.is_some(),
        fields.loading_at.is_some(),
        fields.unloading_at.is_some(),
        fields.notes.is_some(),
    );
}

/// 抽出対象でない (PDF 以外 / GEMINI_API_KEY 未設定) ケース用の薄いショートカット。
///
/// 既存 `extracted_data` から `logistics` キーだけ削除して update_extraction を呼ぶ。
/// 他の extract サブシステムが書いたキーは保持する。
async fn write_no_logistics(
    docs: &dyn NotifyDocumentRepository,
    tenant_id: Uuid,
    document_id: Uuid,
    doc: &alc_core::repository::notify_documents::NotifyDocument,
) {
    let mut data = doc.extracted_data.clone().unwrap_or(serde_json::json!({}));
    if let Some(obj) = data.as_object_mut() {
        obj.remove("logistics");
    }
    let result = ExtractionResult {
        title: doc.extracted_title.clone(),
        date: doc.extracted_date,
        summary: doc.extracted_summary.clone(),
        phone_numbers: doc.extracted_phone_numbers.clone().unwrap_or_default(),
        data,
    };
    if let Err(e) = docs
        .update_extraction(tenant_id, document_id, &result)
        .await
    {
        tracing::error!("extract: update_extraction (no-logistics) failed: {e}");
    }
}

/// `extracted_data` JSONB に `logistics` キーをマージする pure 関数。
///
/// - `existing` が null/JSON object でなかったら、新規 object として上書き
/// - `fields.has_any() == false` なら `logistics` キーを削除 (前回値の残骸を消す)
/// - それ以外は `logistics` キーに `fields` の serde 表現を上書き保存
pub(crate) fn merge_logistics_into_data(
    existing: Option<serde_json::Value>,
    fields: &LogisticsFields,
) -> serde_json::Value {
    let mut data = match existing {
        Some(v) if v.is_object() => v,
        _ => serde_json::json!({}),
    };
    if let Some(obj) = data.as_object_mut() {
        if fields.has_any() {
            obj.insert(
                "logistics".into(),
                serde_json::to_value(fields).unwrap_or(serde_json::Value::Null),
            );
        } else {
            obj.remove("logistics");
        }
    }
    data
}

/// fire-and-forget で background extract を起動する。呼び出し元はすぐ return できる。
///
/// `AppState` から env と repo/storage を取り出して `extract_document_with_deps` に委譲。
/// upload / ingest の各エンドポイントから新規 document の INSERT 直後に呼ぶ。
pub fn spawn_extract_document(state: AppState, tenant_id: Uuid, document_id: Uuid) {
    let api_key = std::env::var("GEMINI_API_KEY").ok();
    let docs: Arc<dyn NotifyDocumentRepository> = state.notify_documents.clone();
    let storage: Option<Arc<dyn StorageBackend>> = state.notify_storage.clone();

    tokio::spawn(async move {
        extract_document_with_deps(
            docs.as_ref(),
            storage.as_deref(),
            api_key.as_deref(),
            None,
            None,
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
        CreateNotifyDocument, ExtractionResult, NotifyDocument,
    };
    use alc_core::storage::StorageError;
    use async_trait::async_trait;

    // ============================================================
    // In-memory test stubs (background_redaction.rs のものを最小化)
    // ============================================================

    #[derive(Default)]
    struct StubDocs {
        extractions: Mutex<Vec<ExtractionResult>>,
        errors: Mutex<Vec<String>>,
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
            r: &ExtractionResult,
        ) -> Result<(), sqlx::Error> {
            self.extractions.lock().unwrap().push(ExtractionResult {
                title: r.title.clone(),
                date: r.date,
                summary: r.summary.clone(),
                phone_numbers: r.phone_numbers.clone(),
                data: r.data.clone(),
            });
            Ok(())
        }
        async fn update_extraction_error(
            &self,
            _t: Uuid,
            _id: Uuid,
            e: &str,
        ) -> Result<(), sqlx::Error> {
            self.errors.lock().unwrap().push(e.to_string());
            Ok(())
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
            _s: &str,
            _e: Option<&str>,
        ) -> Result<(), sqlx::Error> {
            unimplemented!()
        }
        async fn complete_redaction(
            &self,
            _t: Uuid,
            _id: Uuid,
            _k: &str,
            _a: i32,
        ) -> Result<(), sqlx::Error> {
            unimplemented!()
        }
        async fn reset_redaction(&self, _t: Uuid, _id: Uuid) -> Result<(), sqlx::Error> {
            unimplemented!()
        }
    }

    struct StubStorage {
        pdf: Vec<u8>,
        download_should_fail: bool,
    }

    impl StubStorage {
        fn ok(pdf: Vec<u8>) -> Arc<Self> {
            Arc::new(Self {
                pdf,
                download_should_fail: false,
            })
        }
        fn with_download_fail() -> Arc<Self> {
            Arc::new(Self {
                pdf: Vec::new(),
                download_should_fail: true,
            })
        }
    }

    #[async_trait]
    impl StorageBackend for StubStorage {
        async fn upload(
            &self,
            _key: &str,
            _bytes: &[u8],
            _content_type: &str,
        ) -> Result<String, StorageError> {
            unimplemented!()
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
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    /// Gemini モック: schema-conforming JSON を text part に詰める
    async fn start_gemini_mock_returning(json_text: &str) -> wiremock::MockServer {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "candidates": [{
                        "content": {"parts": [{"text": json_text}]}
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
    // merge_logistics_into_data (pure)
    // ============================================================

    #[test]
    fn merge_inserts_logistics_when_has_any() {
        let fields = LogisticsFields {
            loading_place: Some("東京".into()),
            ..Default::default()
        };
        let merged = merge_logistics_into_data(None, &fields);
        let logistics = merged.get("logistics").unwrap();
        assert_eq!(logistics["loading_place"], "東京");
    }

    #[test]
    fn merge_removes_logistics_when_all_empty() {
        let existing =
            serde_json::json!({"logistics": {"loading_place": "old"}, "other_key": "kept"});
        let fields = LogisticsFields::default();
        let merged = merge_logistics_into_data(Some(existing), &fields);
        assert!(
            merged.get("logistics").is_none(),
            "logistics should be cleared"
        );
        assert_eq!(merged.get("other_key").unwrap(), "kept");
    }

    #[test]
    fn merge_preserves_other_keys() {
        let existing = serde_json::json!({"phone_numbers_ext": ["090-..."]});
        let fields = LogisticsFields {
            unloading_place: Some("大阪".into()),
            ..Default::default()
        };
        let merged = merge_logistics_into_data(Some(existing), &fields);
        assert!(merged.get("phone_numbers_ext").is_some());
        assert_eq!(merged["logistics"]["unloading_place"], "大阪");
    }

    #[test]
    fn merge_resets_when_existing_not_object() {
        let existing = serde_json::json!("oops not an object");
        let fields = LogisticsFields {
            notes: Some("hi".into()),
            ..Default::default()
        };
        let merged = merge_logistics_into_data(Some(existing), &fields);
        // 既存値が捨てられて新規 object になる
        assert!(merged.is_object());
        assert_eq!(merged["logistics"]["notes"], "hi");
    }

    // ============================================================
    // extract_document_with_deps (integration)
    // ============================================================

    #[tokio::test]
    async fn extract_completes_and_writes_logistics() {
        let docs = StubDocs::new(build_doc(Some("haisou.pdf")));
        let storage = StubStorage::ok(b"%PDF-1.4 dummy".to_vec());
        let server = start_gemini_mock_returning(
            "{\"loading_place\":\"東京\",\"unloading_place\":\"大阪\",\
             \"loading_at\":\"5/9 10:00\",\"unloading_at\":\"5/10 14:00\",\
             \"notes\":\"急ぎ\"}",
        )
        .await;

        extract_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            Some("test-key"),
            Some(&server.uri()),
            Some("test-model"),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let extractions = docs.extractions.lock().unwrap();
        assert_eq!(extractions.len(), 1);
        let logistics = extractions[0]
            .data
            .get("logistics")
            .expect("logistics key written");
        assert_eq!(logistics["loading_place"], "東京");
        assert_eq!(logistics["notes"], "急ぎ");
        assert!(docs.errors.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn extract_all_null_marks_completed_without_logistics() {
        let docs = StubDocs::new(build_doc(Some("invoice.pdf")));
        let storage = StubStorage::ok(b"%PDF".to_vec());
        let server = start_gemini_mock_returning(
            "{\"loading_place\":null,\"unloading_place\":null,\
             \"loading_at\":null,\"unloading_at\":null,\"notes\":null}",
        )
        .await;

        extract_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            Some("k"),
            Some(&server.uri()),
            Some("m"),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let extractions = docs.extractions.lock().unwrap();
        assert_eq!(extractions.len(), 1);
        // 全 null → logistics キーは追加されない
        assert!(extractions[0].data.get("logistics").is_none());
    }

    #[tokio::test]
    async fn extract_non_pdf_marks_completed_without_logistics() {
        let docs = StubDocs::new(build_doc(Some("photo.jpg")));
        let storage = StubStorage::ok(b"x".to_vec());

        extract_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            Some("k"),
            None,
            None,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let extractions = docs.extractions.lock().unwrap();
        assert_eq!(extractions.len(), 1);
        assert!(extractions[0].data.get("logistics").is_none());
        assert!(docs.errors.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn extract_no_file_name_treated_as_non_pdf() {
        let docs = StubDocs::new(build_doc(None));
        let storage = StubStorage::ok(b"x".to_vec());

        extract_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            Some("k"),
            None,
            None,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let extractions = docs.extractions.lock().unwrap();
        assert_eq!(extractions.len(), 1);
        assert!(extractions[0].data.get("logistics").is_none());
    }

    #[tokio::test]
    async fn extract_no_api_key_marks_completed_without_logistics() {
        let docs = StubDocs::new(build_doc(Some("doc.pdf")));
        let storage = StubStorage::ok(b"%PDF".to_vec());

        extract_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            None,
            None,
            None,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let extractions = docs.extractions.lock().unwrap();
        assert_eq!(extractions.len(), 1);
        // 設定エラーは per-document には残さない (全 doc 共通の問題なので Cloud Run ログ側へ)
        assert!(docs.errors.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn extract_empty_api_key_treated_as_unset() {
        let docs = StubDocs::new(build_doc(Some("doc.pdf")));
        let storage = StubStorage::ok(b"%PDF".to_vec());

        extract_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            Some(""),
            None,
            None,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let extractions = docs.extractions.lock().unwrap();
        assert_eq!(extractions.len(), 1);
    }

    #[tokio::test]
    async fn extract_no_storage_marks_failed() {
        let docs = StubDocs::new(build_doc(Some("doc.pdf")));

        extract_document_with_deps(
            docs.as_ref(),
            None,
            Some("k"),
            None,
            None,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let errors = docs.errors.lock().unwrap();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("notify_storage"));
    }

    #[tokio::test]
    async fn extract_document_not_found_is_noop() {
        let docs = StubDocs::with_no_doc();

        extract_document_with_deps(
            docs.as_ref(),
            None,
            Some("k"),
            None,
            None,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        assert!(docs.extractions.lock().unwrap().is_empty());
        assert!(docs.errors.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn extract_r2_download_failure_marks_failed() {
        let docs = StubDocs::new(build_doc(Some("doc.pdf")));
        let storage = StubStorage::with_download_fail();

        extract_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            Some("k"),
            None,
            None,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let errors = docs.errors.lock().unwrap();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("r2 download"));
    }

    #[tokio::test]
    async fn extract_gemini_5xx_marks_failed_with_gemini_prefix() {
        let docs = StubDocs::new(build_doc(Some("doc.pdf")));
        let storage = StubStorage::ok(b"%PDF".to_vec());
        let server = start_gemini_mock_with_status(500).await;

        extract_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            Some("k"),
            Some(&server.uri()),
            Some("m"),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let errors = docs.errors.lock().unwrap();
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].starts_with("gemini:"),
            "expected gemini: prefix, got: {}",
            errors[0]
        );
    }

    #[tokio::test]
    async fn extract_gemini_inner_text_unparseable_marks_failed_with_parse_prefix() {
        let docs = StubDocs::new(build_doc(Some("doc.pdf")));
        let storage = StubStorage::ok(b"%PDF".to_vec());
        let server = start_gemini_mock_returning("not a json").await;

        extract_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            Some("k"),
            Some(&server.uri()),
            Some("m"),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let errors = docs.errors.lock().unwrap();
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].starts_with("parse:"),
            "expected parse: prefix, got: {}",
            errors[0]
        );
    }

    #[tokio::test]
    async fn extract_preserves_existing_extracted_fields() {
        // 既存 extracted_title / extracted_data の他キーを潰さない
        let mut doc = build_doc(Some("doc.pdf"));
        doc.extracted_title = Some("Existing Title".into());
        doc.extracted_data = Some(serde_json::json!({"phone_numbers_ext": ["090-..."]}));
        let docs = StubDocs::new(doc);
        let storage = StubStorage::ok(b"%PDF".to_vec());
        let server = start_gemini_mock_returning(
            "{\"loading_place\":\"成田\",\"unloading_place\":null,\
             \"loading_at\":null,\"unloading_at\":null,\"notes\":null}",
        )
        .await;

        extract_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            Some("k"),
            Some(&server.uri()),
            Some("m"),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let extractions = docs.extractions.lock().unwrap();
        assert_eq!(extractions[0].title.as_deref(), Some("Existing Title"));
        // 既存キー保持 + logistics 追加
        assert_eq!(extractions[0].data["phone_numbers_ext"][0], "090-...");
        assert_eq!(extractions[0].data["logistics"]["loading_place"], "成田");
    }

    #[tokio::test]
    async fn extract_recompute_clears_old_logistics_when_now_empty() {
        // 既存 extracted_data.logistics があって、再抽出で全 null → logistics キー削除
        let mut doc = build_doc(Some("doc.pdf"));
        doc.extracted_data = Some(serde_json::json!({
            "logistics": {"loading_place": "old value"}
        }));
        let docs = StubDocs::new(doc);
        let storage = StubStorage::ok(b"%PDF".to_vec());
        let server = start_gemini_mock_returning(
            "{\"loading_place\":null,\"unloading_place\":null,\
             \"loading_at\":null,\"unloading_at\":null,\"notes\":null}",
        )
        .await;

        extract_document_with_deps(
            docs.as_ref(),
            Some(storage.as_ref() as &dyn StorageBackend),
            Some("k"),
            Some(&server.uri()),
            Some("m"),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        let extractions = docs.extractions.lock().unwrap();
        assert!(extractions[0].data.get("logistics").is_none());
    }
}

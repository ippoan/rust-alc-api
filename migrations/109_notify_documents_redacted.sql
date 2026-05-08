-- notify_documents: redact 結果 + ステータス管理用カラム追加。
-- redact は upload / mail ingest 時に tokio::spawn で非同期実行され、
-- 結果は redaction_status カラムで追跡する。
--
-- redaction_status 値:
--   'pending'    : 初期値、まだ background ジョブが触っていない
--   'processing' : tokio::spawn 開始
--   'completed'  : redact 成功 (redacted_r2_key が set されている)
--   'skipped'    : PDF 以外 or GEMINI_API_KEY 未設定 (redact 不要扱い、配信は許可)
--   'failed'     : Gemini or apply_redactions エラー (配信ブロック対象)
ALTER TABLE alc_api.notify_documents
  ADD COLUMN IF NOT EXISTS redacted_r2_key TEXT,
  ADD COLUMN IF NOT EXISTS redacted_at TIMESTAMPTZ,
  ADD COLUMN IF NOT EXISTS redactions_applied INTEGER,
  ADD COLUMN IF NOT EXISTS redaction_status TEXT NOT NULL DEFAULT 'pending',
  ADD COLUMN IF NOT EXISTS redaction_error TEXT;

-- 公開 viewer は redacted があればそちらを返す。
-- nuxt-notify の /v/{token} ページ + LINE/LINE WORKS の受信者は
-- read_token 経由で常に COALESCE 結果を取得する。
-- 既存 admin auth の preview エンドポイントは redacted_r2_key を直接見るので影響なし。
CREATE OR REPLACE FUNCTION alc_api.lookup_delivery_for_view(p_read_token UUID)
RETURNS TABLE(
    document_id UUID,
    tenant_id UUID,
    r2_key TEXT,
    file_name TEXT,
    file_size_bytes BIGINT,
    source_subject TEXT,
    source_sender TEXT,
    source_received_at TIMESTAMPTZ,
    expire_at TIMESTAMPTZ
)
LANGUAGE sql SECURITY DEFINER SET search_path = alc_api
AS $$
    SELECT d.document_id, d.tenant_id,
           COALESCE(doc.redacted_r2_key, doc.r2_key) AS r2_key,
           doc.file_name, doc.file_size_bytes,
           doc.source_subject, doc.source_sender, doc.source_received_at,
           d.expire_at
    FROM alc_api.notify_deliveries d
    JOIN alc_api.notify_documents doc ON doc.id = d.document_id
    WHERE d.read_token = p_read_token
$$;

GRANT EXECUTE ON FUNCTION alc_api.lookup_delivery_for_view(UUID) TO alc_api_app;

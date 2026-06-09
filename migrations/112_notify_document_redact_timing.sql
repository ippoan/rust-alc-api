-- notify_documents: redact パイプラインの stage 別レイテンシ (ms) を保存する。
-- background_redaction.rs が計測する dl_ms / llm_ms / render_ms / up_ms / total_ms を
-- complete_redaction 時に書き込む。nuxt-notify の documents/[id].vue で個別 document の
-- 遅延内訳をデバッグ表示するための列 (Cloud Logging を引かずに済む)。Refs #334 / #333。
--
-- NULL 許容: migration 適用前に redact 済みの旧データは NULL のまま。
-- 検索ではなく表示用なのでインデックスは張らない。
ALTER TABLE alc_api.notify_documents
  ADD COLUMN IF NOT EXISTS redact_dl_ms     INTEGER,
  ADD COLUMN IF NOT EXISTS redact_llm_ms    INTEGER,
  ADD COLUMN IF NOT EXISTS redact_render_ms INTEGER,
  ADD COLUMN IF NOT EXISTS redact_upload_ms INTEGER,
  ADD COLUMN IF NOT EXISTS redact_total_ms  INTEGER;

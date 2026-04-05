-- LINE Login 対応: users テーブルに line_user_id 追加 + recipient 逆引き関数

-- users テーブルに line_user_id 追加
ALTER TABLE alc_api.users ADD COLUMN IF NOT EXISTS line_user_id TEXT;
CREATE UNIQUE INDEX IF NOT EXISTS users_line_user_id_unique
  ON alc_api.users (line_user_id) WHERE line_user_id IS NOT NULL;

-- CHECK 制約を更新 (google_sub OR lineworks_id OR line_user_id)
ALTER TABLE alc_api.users DROP CONSTRAINT IF EXISTS user_has_provider;
ALTER TABLE alc_api.users ADD CONSTRAINT user_has_provider
  CHECK (google_sub IS NOT NULL OR lineworks_id IS NOT NULL OR line_user_id IS NOT NULL);

-- RLS バイパスで notify_recipients.line_user_id → tenant_id を解決する SECURITY DEFINER 関数
CREATE OR REPLACE FUNCTION alc_api.find_recipient_by_line_user_id(p_line_user_id TEXT)
RETURNS TABLE(tenant_id UUID, recipient_name TEXT)
LANGUAGE sql SECURITY DEFINER SET search_path = alc_api AS $$
  SELECT tenant_id, name FROM alc_api.notify_recipients
  WHERE line_user_id = p_line_user_id AND enabled = TRUE
  LIMIT 1;
$$;

GRANT EXECUTE ON FUNCTION alc_api.find_recipient_by_line_user_id TO alc_api_app;

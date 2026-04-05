-- notify_line_configs を JWT assertion 方式に変更
-- channel_access_token_encrypted → private_key_encrypted + key_id

ALTER TABLE alc_api.notify_line_configs
    ADD COLUMN key_id TEXT,
    ADD COLUMN private_key_encrypted TEXT;

-- channel_access_token_encrypted は不要になるが、既存データ保護のため残す (nullable)
-- 新規登録では key_id + private_key_encrypted を使う

-- lookup 関数を更新 (戻り値型が変わるため DROP → CREATE)
DROP FUNCTION IF EXISTS alc_api.lookup_line_config_by_channel(TEXT);
CREATE OR REPLACE FUNCTION alc_api.lookup_line_config_by_channel(p_channel_id TEXT)
RETURNS TABLE(id UUID, tenant_id UUID, channel_secret_encrypted TEXT, channel_access_token_encrypted TEXT, key_id TEXT, private_key_encrypted TEXT)
LANGUAGE sql SECURITY DEFINER SET search_path = alc_api
AS $$
    SELECT id, tenant_id, channel_secret_encrypted, channel_access_token_encrypted, key_id, private_key_encrypted
    FROM alc_api.notify_line_configs
    WHERE channel_id = p_channel_id AND enabled = TRUE;
$$;

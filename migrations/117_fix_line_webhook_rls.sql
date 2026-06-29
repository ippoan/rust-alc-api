-- LINE webhook 署名照合の RLS 500 修正 (Refs #434)
--
-- `find_config_by_signature` は LINE webhook の未認証パスで全テナントの enabled config を
-- 列挙して署名 (X-Line-Signature) を照合する。ところが生クエリ (RLS 対象) で引いていたため、
-- pooled connection に `app.current_tenant_id` が空文字 ('') で残っていると RLS ポリシー
-- (`tenant_id = current_setting('app.current_tenant_id', true)::UUID`) の `''::UUID` キャストが
-- "invalid input syntax for type uuid" で落ち、クエリごと 500 になっていた。
--
-- 認証前アクセス用の `lookup_line_config_by_channel` (072/074) と同じく SECURITY DEFINER で
-- RLS をバイパスして列挙する関数を用意し、handler 側はこれを呼ぶ (devices テーブルの
-- `lookup_device_tenant` 置換と同パターン)。値の取得は signature 照合に必要な列のみ。
CREATE OR REPLACE FUNCTION alc_api.list_enabled_line_configs()
RETURNS TABLE(
    tenant_id UUID,
    channel_id TEXT,
    channel_secret_encrypted TEXT,
    key_id TEXT,
    private_key_encrypted TEXT
)
LANGUAGE sql SECURITY DEFINER SET search_path = alc_api
AS $$
    SELECT tenant_id, channel_id, channel_secret_encrypted, key_id, private_key_encrypted
    FROM alc_api.notify_line_configs
    WHERE enabled = TRUE;
$$;

GRANT EXECUTE ON FUNCTION alc_api.list_enabled_line_configs() TO alc_api_app;

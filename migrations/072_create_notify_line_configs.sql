-- notify_line_configs: LINE Messaging API 設定 (テナントごと)
CREATE TABLE alc_api.notify_line_configs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    channel_secret_encrypted TEXT NOT NULL,
    channel_access_token_encrypted TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id)
);

-- RLS: テナント分離
ALTER TABLE alc_api.notify_line_configs ENABLE ROW LEVEL SECURITY;
ALTER TABLE alc_api.notify_line_configs FORCE ROW LEVEL SECURITY;

CREATE POLICY notify_line_configs_select ON alc_api.notify_line_configs
    FOR SELECT USING (tenant_id = current_setting('app.current_tenant_id', true)::UUID);
CREATE POLICY notify_line_configs_insert ON alc_api.notify_line_configs
    FOR INSERT WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::UUID);
CREATE POLICY notify_line_configs_update ON alc_api.notify_line_configs
    FOR UPDATE USING (tenant_id = current_setting('app.current_tenant_id', true)::UUID);
CREATE POLICY notify_line_configs_delete ON alc_api.notify_line_configs
    FOR DELETE USING (tenant_id = current_setting('app.current_tenant_id', true)::UUID);

-- LINE webhook: channel_id からテナント特定 (認証前アクセス用)
CREATE OR REPLACE FUNCTION alc_api.lookup_line_config_by_channel(p_channel_id TEXT)
RETURNS TABLE(id UUID, tenant_id UUID, channel_secret_encrypted TEXT, channel_access_token_encrypted TEXT)
LANGUAGE sql SECURITY DEFINER SET search_path = alc_api
AS $$
    SELECT id, tenant_id, channel_secret_encrypted, channel_access_token_encrypted
    FROM alc_api.notify_line_configs
    WHERE channel_id = p_channel_id AND enabled = TRUE;
$$;

GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.notify_line_configs TO alc_api_app;
GRANT EXECUTE ON FUNCTION alc_api.lookup_line_config_by_channel(TEXT) TO alc_api_app;

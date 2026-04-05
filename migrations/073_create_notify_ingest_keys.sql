-- notify_ingest_keys: Email Worker の API キー認証
CREATE TABLE alc_api.notify_ingest_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id) ON DELETE CASCADE,
    key_hash TEXT NOT NULL,
    name TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- RLS: テナント分離 (管理画面での一覧用)
ALTER TABLE alc_api.notify_ingest_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE alc_api.notify_ingest_keys FORCE ROW LEVEL SECURITY;

CREATE POLICY notify_ingest_keys_select ON alc_api.notify_ingest_keys
    FOR SELECT USING (tenant_id = current_setting('app.current_tenant_id', true)::UUID);
CREATE POLICY notify_ingest_keys_insert ON alc_api.notify_ingest_keys
    FOR INSERT WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::UUID);
CREATE POLICY notify_ingest_keys_delete ON alc_api.notify_ingest_keys
    FOR DELETE USING (tenant_id = current_setting('app.current_tenant_id', true)::UUID);

-- Email Worker 認証: key_hash でテナント特定 (認証前アクセス用)
CREATE OR REPLACE FUNCTION alc_api.verify_ingest_key(p_key_hash TEXT)
RETURNS UUID
LANGUAGE sql SECURITY DEFINER SET search_path = alc_api
AS $$
    SELECT tenant_id FROM alc_api.notify_ingest_keys
    WHERE key_hash = p_key_hash AND enabled = TRUE
    LIMIT 1;
$$;

GRANT SELECT, INSERT, DELETE ON alc_api.notify_ingest_keys TO alc_api_app;
GRANT EXECUTE ON FUNCTION alc_api.verify_ingest_key(TEXT) TO alc_api_app;

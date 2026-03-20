-- Bot configurations per tenant (LINE WORKS Bot 等)
CREATE TABLE IF NOT EXISTS alc_api.bot_configs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id) ON DELETE CASCADE,
    provider TEXT NOT NULL DEFAULT 'lineworks',
    name TEXT NOT NULL,
    client_id TEXT NOT NULL,
    client_secret_encrypted TEXT NOT NULL,
    service_account TEXT NOT NULL,
    private_key_encrypted TEXT NOT NULL,
    bot_id TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_bot_configs_provider_bot_id ON alc_api.bot_configs(provider, bot_id);
CREATE INDEX IF NOT EXISTS idx_bot_configs_tenant ON alc_api.bot_configs(tenant_id) WHERE enabled = TRUE;

ALTER TABLE alc_api.bot_configs ENABLE ROW LEVEL SECURITY;
ALTER TABLE alc_api.bot_configs FORCE ROW LEVEL SECURITY;

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_policies WHERE tablename = 'bot_configs' AND schemaname = 'alc_api' AND policyname = 'tenant_isolation') THEN
        CREATE POLICY tenant_isolation ON alc_api.bot_configs
            USING (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ))
            WITH CHECK (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ));
    END IF;
END $$;

GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.bot_configs TO alc_api_app;

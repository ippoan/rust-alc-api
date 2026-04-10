-- trouble_categories
CREATE TABLE alc_api.trouble_categories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    name TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(tenant_id, name)
);
ALTER TABLE alc_api.trouble_categories ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON alc_api.trouble_categories
    FOR ALL USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);
GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.trouble_categories TO alc_api_app;

-- trouble_offices
CREATE TABLE alc_api.trouble_offices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    name TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(tenant_id, name)
);
ALTER TABLE alc_api.trouble_offices ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON alc_api.trouble_offices
    FOR ALL USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);
GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.trouble_offices TO alc_api_app;

-- registration_number column
ALTER TABLE alc_api.trouble_tickets ADD COLUMN IF NOT EXISTS registration_number TEXT NOT NULL DEFAULT '';

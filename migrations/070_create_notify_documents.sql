-- notify_documents: 配信ドキュメント (PDF等)
CREATE TABLE alc_api.notify_documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id) ON DELETE CASCADE,
    source_type TEXT NOT NULL DEFAULT 'email',
    source_sender TEXT,
    source_subject TEXT,
    r2_key TEXT NOT NULL,
    file_name TEXT,
    file_size_bytes BIGINT,
    extracted_title TEXT,
    extracted_date DATE,
    extracted_summary TEXT,
    extracted_phone_numbers TEXT[],
    extracted_data JSONB,
    extraction_status TEXT NOT NULL DEFAULT 'pending',
    extraction_error TEXT,
    distribution_status TEXT NOT NULL DEFAULT 'pending',
    distributed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- RLS: テナント分離
ALTER TABLE alc_api.notify_documents ENABLE ROW LEVEL SECURITY;
ALTER TABLE alc_api.notify_documents FORCE ROW LEVEL SECURITY;

CREATE POLICY notify_documents_select ON alc_api.notify_documents
    FOR SELECT USING (tenant_id = current_setting('app.current_tenant_id', true)::UUID);
CREATE POLICY notify_documents_insert ON alc_api.notify_documents
    FOR INSERT WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::UUID);
CREATE POLICY notify_documents_update ON alc_api.notify_documents
    FOR UPDATE USING (tenant_id = current_setting('app.current_tenant_id', true)::UUID);
CREATE POLICY notify_documents_delete ON alc_api.notify_documents
    FOR DELETE USING (tenant_id = current_setting('app.current_tenant_id', true)::UUID);

CREATE INDEX idx_notify_documents_tenant_created ON alc_api.notify_documents(tenant_id, created_at DESC);
CREATE INDEX idx_notify_documents_extracted_date ON alc_api.notify_documents(extracted_date);

GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.notify_documents TO alc_api_app;

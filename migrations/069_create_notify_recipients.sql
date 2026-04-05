-- notify_recipients: メッセージ配信の受信者
CREATE TABLE alc_api.notify_recipients (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    provider TEXT NOT NULL CHECK (provider IN ('lineworks', 'line')),
    lineworks_user_id TEXT,
    line_user_id TEXT,
    phone_number TEXT,
    email TEXT,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT at_least_one_messaging_id CHECK (lineworks_user_id IS NOT NULL OR line_user_id IS NOT NULL)
);

-- RLS: テナント分離
ALTER TABLE alc_api.notify_recipients ENABLE ROW LEVEL SECURITY;
ALTER TABLE alc_api.notify_recipients FORCE ROW LEVEL SECURITY;

CREATE POLICY notify_recipients_select ON alc_api.notify_recipients
    FOR SELECT USING (tenant_id = current_setting('app.current_tenant_id', true)::UUID);
CREATE POLICY notify_recipients_insert ON alc_api.notify_recipients
    FOR INSERT WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::UUID);
CREATE POLICY notify_recipients_update ON alc_api.notify_recipients
    FOR UPDATE USING (tenant_id = current_setting('app.current_tenant_id', true)::UUID);
CREATE POLICY notify_recipients_delete ON alc_api.notify_recipients
    FOR DELETE USING (tenant_id = current_setting('app.current_tenant_id', true)::UUID);

CREATE UNIQUE INDEX idx_notify_recipients_lw ON alc_api.notify_recipients(tenant_id, lineworks_user_id)
    WHERE lineworks_user_id IS NOT NULL;
CREATE UNIQUE INDEX idx_notify_recipients_line ON alc_api.notify_recipients(tenant_id, line_user_id)
    WHERE line_user_id IS NOT NULL;

GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.notify_recipients TO alc_api_app;

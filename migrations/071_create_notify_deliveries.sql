-- notify_deliveries: 配信記録 (受信者ごとの送信状況・既読状況)
CREATE TABLE alc_api.notify_deliveries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id) ON DELETE CASCADE,
    document_id UUID NOT NULL REFERENCES alc_api.notify_documents(id) ON DELETE CASCADE,
    recipient_id UUID NOT NULL REFERENCES alc_api.notify_recipients(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    error_message TEXT,
    attempt INTEGER NOT NULL DEFAULT 0,
    sent_at TIMESTAMPTZ,
    read_at TIMESTAMPTZ,
    read_token UUID NOT NULL DEFAULT gen_random_uuid(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- RLS: テナント分離
ALTER TABLE alc_api.notify_deliveries ENABLE ROW LEVEL SECURITY;
ALTER TABLE alc_api.notify_deliveries FORCE ROW LEVEL SECURITY;

CREATE POLICY notify_deliveries_select ON alc_api.notify_deliveries
    FOR SELECT USING (tenant_id = current_setting('app.current_tenant_id', true)::UUID);
CREATE POLICY notify_deliveries_insert ON alc_api.notify_deliveries
    FOR INSERT WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::UUID);
CREATE POLICY notify_deliveries_update ON alc_api.notify_deliveries
    FOR UPDATE USING (tenant_id = current_setting('app.current_tenant_id', true)::UUID);

CREATE UNIQUE INDEX idx_notify_deliveries_read_token ON alc_api.notify_deliveries(read_token);
CREATE INDEX idx_notify_deliveries_document ON alc_api.notify_deliveries(document_id);
CREATE INDEX idx_notify_deliveries_recipient ON alc_api.notify_deliveries(recipient_id);

-- 既読トラッキング: read_token で RLS バイパスが必要 (public endpoint)
CREATE OR REPLACE FUNCTION alc_api.mark_delivery_read(p_read_token UUID)
RETURNS TABLE(document_id UUID, tenant_id UUID)
LANGUAGE sql SECURITY DEFINER SET search_path = alc_api
AS $$
    UPDATE alc_api.notify_deliveries
    SET read_at = NOW()
    WHERE read_token = p_read_token AND read_at IS NULL
    RETURNING document_id, tenant_id;
$$;

GRANT SELECT, INSERT, UPDATE ON alc_api.notify_deliveries TO alc_api_app;
GRANT EXECUTE ON FUNCTION alc_api.mark_delivery_read(UUID) TO alc_api_app;

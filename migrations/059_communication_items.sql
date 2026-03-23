-- 059: 伝達事項テーブル
-- 運行管理者が遠隔点呼時に運転者に伝達すべき事項を管理

CREATE TABLE IF NOT EXISTS alc_api.communication_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    title TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    priority TEXT NOT NULL DEFAULT 'normal',
    target_employee_id UUID REFERENCES alc_api.employees(id),
    is_active BOOLEAN NOT NULL DEFAULT true,
    effective_from TIMESTAMPTZ,
    effective_until TIMESTAMPTZ,
    created_by TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE alc_api.communication_items ENABLE ROW LEVEL SECURITY;
CREATE POLICY communication_items_tenant ON alc_api.communication_items
    USING (tenant_id = current_setting('app.current_tenant_id')::UUID);

COMMENT ON TABLE alc_api.communication_items IS '遠隔点呼時の伝達事項';
COMMENT ON COLUMN alc_api.communication_items.priority IS '優先度 (urgent/normal/low)';
COMMENT ON COLUMN alc_api.communication_items.target_employee_id IS '対象者 (NULL=全員)';

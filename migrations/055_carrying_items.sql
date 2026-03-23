-- 055: 携行品マスタ + 点呼セッション携行品チェック

-- 携行品マスタ（テナントごと）
CREATE TABLE IF NOT EXISTS alc_api.carrying_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    item_name TEXT NOT NULL,
    is_required BOOLEAN NOT NULL DEFAULT true,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
ALTER TABLE alc_api.carrying_items ENABLE ROW LEVEL SECURITY;
CREATE POLICY carrying_items_tenant ON alc_api.carrying_items
    USING (tenant_id = current_setting('app.current_tenant_id')::UUID);

-- セッションごとの携行品チェック結果
CREATE TABLE IF NOT EXISTS alc_api.tenko_carrying_item_checks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID NOT NULL REFERENCES alc_api.tenko_sessions(id),
    item_id UUID NOT NULL REFERENCES alc_api.carrying_items(id),
    item_name TEXT NOT NULL,
    checked BOOLEAN NOT NULL DEFAULT false,
    checked_at TIMESTAMPTZ,
    UNIQUE(session_id, item_id)
);
ALTER TABLE alc_api.tenko_carrying_item_checks ENABLE ROW LEVEL SECURITY;
CREATE POLICY carrying_item_checks_tenant ON alc_api.tenko_carrying_item_checks
    USING (
        session_id IN (SELECT id FROM alc_api.tenko_sessions WHERE tenant_id = current_setting('app.current_tenant_id')::UUID)
    );

-- tenko_sessions に携行品チェック結果サマリ追加
ALTER TABLE alc_api.tenko_sessions ADD COLUMN IF NOT EXISTS carrying_items_checked JSONB;

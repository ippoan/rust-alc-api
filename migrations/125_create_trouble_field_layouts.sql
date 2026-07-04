-- トラブルチケット入力フォームの表示/非表示・幅・並び順をテナント単位で保持する。
-- フィールド定義自体 (label 等) はフロントエンドが持ち、ここには各 field_key に対する
-- 上書き設定 (visible/width/sort_order) の配列だけを JSONB で保持する。
CREATE TABLE alc_api.trouble_field_layouts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id) ON DELETE CASCADE,
    settings JSONB NOT NULL DEFAULT '[]',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(tenant_id)
);

ALTER TABLE alc_api.trouble_field_layouts ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON alc_api.trouble_field_layouts
    FOR ALL USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.trouble_field_layouts TO alc_api_app;

-- Items: 物品管理 (nuxt-items フロントエンド用)
-- 階層構造 (parentId でフォルダ/アイテムのツリー)
-- ownerType で組織/個人を区別

CREATE TABLE alc_api.items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    parent_id UUID REFERENCES alc_api.items(id) ON DELETE CASCADE,
    owner_type TEXT NOT NULL DEFAULT 'org' CHECK (owner_type IN ('org', 'personal')),
    owner_user_id UUID,
    item_type TEXT NOT NULL DEFAULT 'item' CHECK (item_type IN ('item', 'folder')),
    name TEXT NOT NULL,
    barcode TEXT NOT NULL DEFAULT '',
    category TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    image_url TEXT NOT NULL DEFAULT '',
    url TEXT NOT NULL DEFAULT '',
    quantity INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_items_tenant_parent ON alc_api.items(tenant_id, parent_id);
CREATE INDEX idx_items_tenant_barcode ON alc_api.items(tenant_id, barcode) WHERE barcode != '';

-- Files: 物品画像等のファイルメタデータ
-- 実体は R2 (alc-items-files バケット)
CREATE TABLE alc_api.item_files (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    filename TEXT NOT NULL DEFAULT '',
    content_type TEXT NOT NULL DEFAULT 'application/octet-stream',
    size_bytes BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- RLS
ALTER TABLE alc_api.items ENABLE ROW LEVEL SECURITY;
ALTER TABLE alc_api.item_files ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_items ON alc_api.items
    FOR ALL
    USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

CREATE POLICY tenant_isolation_item_files ON alc_api.item_files
    FOR ALL
    USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

-- Grants
GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.items TO alc_api_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.item_files TO alc_api_app;

-- トラブル管理テーブル

-- ワークフロー状態定義 (テナントごとにカスタマイズ可能)
CREATE TABLE alc_api.trouble_workflow_states (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    name TEXT NOT NULL,
    label TEXT NOT NULL,
    color TEXT NOT NULL DEFAULT '#6B7280',
    sort_order INTEGER NOT NULL DEFAULT 0,
    is_initial BOOLEAN NOT NULL DEFAULT FALSE,
    is_terminal BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(tenant_id, name)
);

-- 状態遷移ルール
CREATE TABLE alc_api.trouble_workflow_transitions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    from_state_id UUID NOT NULL REFERENCES alc_api.trouble_workflow_states(id) ON DELETE CASCADE,
    to_state_id UUID NOT NULL REFERENCES alc_api.trouble_workflow_states(id) ON DELETE CASCADE,
    label TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(tenant_id, from_state_id, to_state_id)
);

-- トラブルチケット本体
CREATE TABLE alc_api.trouble_tickets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    ticket_no SERIAL,
    -- 基本情報
    category TEXT NOT NULL,
    title TEXT NOT NULL DEFAULT '',
    occurred_at TIMESTAMPTZ,
    occurred_date DATE,
    company_name TEXT NOT NULL DEFAULT '',
    office_name TEXT NOT NULL DEFAULT '',
    department TEXT NOT NULL DEFAULT '',
    person_name TEXT NOT NULL DEFAULT '',
    person_id UUID REFERENCES alc_api.employees(id),
    vehicle_number TEXT NOT NULL DEFAULT '',
    location TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    -- ワークフロー
    status_id UUID REFERENCES alc_api.trouble_workflow_states(id),
    assigned_to UUID,
    -- 進捗・金額
    progress_notes TEXT NOT NULL DEFAULT '',
    allowance TEXT NOT NULL DEFAULT '',
    damage_amount NUMERIC(12,2),
    compensation_amount NUMERIC(12,2),
    confirmation_notice TEXT NOT NULL DEFAULT '',
    disciplinary_content TEXT NOT NULL DEFAULT '',
    road_service_cost NUMERIC(12,2),
    -- 相手方情報
    counterparty TEXT NOT NULL DEFAULT '',
    counterparty_insurance TEXT NOT NULL DEFAULT '',
    -- カスタムフィールド
    custom_fields JSONB NOT NULL DEFAULT '{}',
    -- 期限
    due_date TIMESTAMPTZ,
    overdue_notified_at TIMESTAMPTZ,
    -- メタデータ
    created_by UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

CREATE INDEX idx_trouble_tickets_tenant ON alc_api.trouble_tickets(tenant_id);
CREATE INDEX idx_trouble_tickets_tenant_category ON alc_api.trouble_tickets(tenant_id, category);
CREATE INDEX idx_trouble_tickets_tenant_status ON alc_api.trouble_tickets(tenant_id, status_id);
CREATE INDEX idx_trouble_tickets_occurred ON alc_api.trouble_tickets(tenant_id, occurred_date);
CREATE INDEX idx_trouble_tickets_active ON alc_api.trouble_tickets(tenant_id) WHERE deleted_at IS NULL;

-- チケット添付ファイル
CREATE TABLE alc_api.trouble_files (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    ticket_id UUID NOT NULL REFERENCES alc_api.trouble_tickets(id) ON DELETE CASCADE,
    filename TEXT NOT NULL,
    content_type TEXT NOT NULL DEFAULT 'application/octet-stream',
    size_bytes BIGINT NOT NULL DEFAULT 0,
    storage_key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_trouble_files_ticket ON alc_api.trouble_files(ticket_id);

-- ステータス変更履歴
CREATE TABLE alc_api.trouble_status_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    ticket_id UUID NOT NULL REFERENCES alc_api.trouble_tickets(id) ON DELETE CASCADE,
    from_state_id UUID REFERENCES alc_api.trouble_workflow_states(id),
    to_state_id UUID NOT NULL REFERENCES alc_api.trouble_workflow_states(id),
    changed_by UUID,
    comment TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_trouble_history_ticket ON alc_api.trouble_status_history(ticket_id);

-- コメント
CREATE TABLE alc_api.trouble_comments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    ticket_id UUID NOT NULL REFERENCES alc_api.trouble_tickets(id) ON DELETE CASCADE,
    author_id UUID,
    body TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_trouble_comments_ticket ON alc_api.trouble_comments(ticket_id);

-- カスタム項目定義
CREATE TABLE alc_api.trouble_custom_field_defs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    field_key TEXT NOT NULL,
    label TEXT NOT NULL,
    field_type TEXT NOT NULL CHECK (field_type IN ('text', 'number', 'date', 'select')),
    options JSONB,
    required BOOLEAN NOT NULL DEFAULT FALSE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(tenant_id, field_key)
);

-- 通知設定
CREATE TABLE alc_api.trouble_notification_prefs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    event_type TEXT NOT NULL,
    notify_channel TEXT NOT NULL CHECK (notify_channel IN ('webhook', 'lineworks', 'line', 'fcm')),
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    recipient_ids UUID[] NOT NULL DEFAULT '{}',
    notify_admins BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(tenant_id, event_type, notify_channel)
);

-- RLS
ALTER TABLE alc_api.trouble_workflow_states ENABLE ROW LEVEL SECURITY;
ALTER TABLE alc_api.trouble_workflow_transitions ENABLE ROW LEVEL SECURITY;
ALTER TABLE alc_api.trouble_tickets ENABLE ROW LEVEL SECURITY;
ALTER TABLE alc_api.trouble_files ENABLE ROW LEVEL SECURITY;
ALTER TABLE alc_api.trouble_status_history ENABLE ROW LEVEL SECURITY;
ALTER TABLE alc_api.trouble_comments ENABLE ROW LEVEL SECURITY;
ALTER TABLE alc_api.trouble_custom_field_defs ENABLE ROW LEVEL SECURITY;
ALTER TABLE alc_api.trouble_notification_prefs ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON alc_api.trouble_workflow_states
    FOR ALL USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

CREATE POLICY tenant_isolation ON alc_api.trouble_workflow_transitions
    FOR ALL USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

CREATE POLICY tenant_isolation ON alc_api.trouble_tickets
    FOR ALL USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

CREATE POLICY tenant_isolation ON alc_api.trouble_files
    FOR ALL USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

CREATE POLICY tenant_isolation ON alc_api.trouble_status_history
    FOR ALL USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

CREATE POLICY tenant_isolation ON alc_api.trouble_comments
    FOR ALL USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

CREATE POLICY tenant_isolation ON alc_api.trouble_custom_field_defs
    FOR ALL USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

CREATE POLICY tenant_isolation ON alc_api.trouble_notification_prefs
    FOR ALL USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

-- Grants
GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.trouble_workflow_states TO alc_api_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.trouble_workflow_transitions TO alc_api_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.trouble_tickets TO alc_api_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.trouble_files TO alc_api_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.trouble_status_history TO alc_api_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.trouble_comments TO alc_api_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.trouble_custom_field_defs TO alc_api_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.trouble_notification_prefs TO alc_api_app;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA alc_api TO alc_api_app;

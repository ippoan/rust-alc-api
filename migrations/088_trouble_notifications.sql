-- Phase 4: トラブル管理 LINE WORKS Bot 通知 + スケジュール通知

-- LINE WORKS user_id (文字列) 格納用
ALTER TABLE trouble_notification_prefs
    ADD COLUMN IF NOT EXISTS lineworks_user_ids TEXT[] NOT NULL DEFAULT '{}';

-- スケジュール通知テーブル
CREATE TABLE alc_api.trouble_schedules (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    ticket_id UUID NOT NULL REFERENCES alc_api.trouble_tickets(id) ON DELETE CASCADE,
    scheduled_at TIMESTAMPTZ NOT NULL,
    message TEXT NOT NULL,
    lineworks_user_ids TEXT[] NOT NULL DEFAULT '{}',
    cloud_task_name TEXT,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'sent', 'cancelled', 'failed')),
    created_by UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    sent_at TIMESTAMPTZ
);

ALTER TABLE alc_api.trouble_schedules ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON alc_api.trouble_schedules
    FOR ALL USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);
GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.trouble_schedules TO alc_api_app;

CREATE INDEX idx_trouble_schedules_ticket ON alc_api.trouble_schedules(ticket_id);
CREATE INDEX idx_trouble_schedules_pending ON alc_api.trouble_schedules(status, scheduled_at)
    WHERE status = 'pending';

-- スケジュール fire 用 SECURITY DEFINER 関数 (Cloud Tasks からの呼び出しは RLS 外)
CREATE OR REPLACE FUNCTION alc_api.get_trouble_schedule(p_id UUID)
RETURNS TABLE (
    id UUID, tenant_id UUID, ticket_id UUID, scheduled_at TIMESTAMPTZ,
    message TEXT, lineworks_user_ids TEXT[], cloud_task_name TEXT,
    status TEXT, created_by UUID, created_at TIMESTAMPTZ, sent_at TIMESTAMPTZ
)
LANGUAGE sql SECURITY DEFINER SET search_path = alc_api AS $$
    SELECT id, tenant_id, ticket_id, scheduled_at, message, lineworks_user_ids,
           cloud_task_name, status, created_by, created_at, sent_at
    FROM alc_api.trouble_schedules WHERE id = p_id AND status = 'pending';
$$;

GRANT EXECUTE ON FUNCTION alc_api.get_trouble_schedule(UUID) TO alc_api_app;

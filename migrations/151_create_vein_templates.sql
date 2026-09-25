-- 指静脈テンプレート (乗務員 1 人 1 件、Refs ippoan/vein-match#20)
-- template は vein-match の Library::get_enroll_template / create_template の出力 (base64)。
-- import_temp_b64 がそのまま読める形。正本はここで、学習後のテンプレートもここへ書き戻す。
CREATE TABLE alc_api.vein_templates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    employee_id UUID NOT NULL REFERENCES alc_api.employees(id),
    template TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, employee_id)
);

ALTER TABLE alc_api.vein_templates ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation_vein_templates ON alc_api.vein_templates
    USING (tenant_id = current_setting('app.current_tenant_id')::UUID);

-- GRANT ON ALL TABLES は既存表のスナップショットなので新表に効かない (忘れると本番で 502)
GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.vein_templates TO alc_api_app;

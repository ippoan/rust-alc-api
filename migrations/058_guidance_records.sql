-- 058: 指導監督の記録テーブル

CREATE TABLE IF NOT EXISTS alc_api.guidance_records (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    employee_id UUID NOT NULL REFERENCES alc_api.employees(id),
    guidance_type TEXT NOT NULL DEFAULT 'general',
    title TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    guided_by TEXT,
    guided_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE alc_api.guidance_records ENABLE ROW LEVEL SECURITY;
CREATE POLICY guidance_records_tenant ON alc_api.guidance_records
    USING (tenant_id = current_setting('app.current_tenant_id')::UUID);

COMMENT ON TABLE alc_api.guidance_records IS '運転者等に対する指導監督の記録';
COMMENT ON COLUMN alc_api.guidance_records.guidance_type IS '指導種別 (general/safety/legal/skill/other)';

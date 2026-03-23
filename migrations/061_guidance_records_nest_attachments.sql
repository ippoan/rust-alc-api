-- 061: 指導監督記録のネスト化 + 添付ファイル

-- parent_id で3階層ネスト (NULL=トップレベル)
ALTER TABLE alc_api.guidance_records ADD COLUMN IF NOT EXISTS parent_id UUID REFERENCES alc_api.guidance_records(id);
ALTER TABLE alc_api.guidance_records ADD COLUMN IF NOT EXISTS depth INTEGER NOT NULL DEFAULT 0;
CREATE INDEX IF NOT EXISTS idx_guidance_records_parent ON alc_api.guidance_records(parent_id);

-- 添付ファイルテーブル
CREATE TABLE IF NOT EXISTS alc_api.guidance_record_attachments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    record_id UUID NOT NULL REFERENCES alc_api.guidance_records(id) ON DELETE CASCADE,
    file_name TEXT NOT NULL,
    file_type TEXT NOT NULL,
    file_size INTEGER,
    storage_url TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_gra_record ON alc_api.guidance_record_attachments(record_id);
ALTER TABLE alc_api.guidance_record_attachments ENABLE ROW LEVEL SECURITY;
CREATE POLICY gra_tenant ON alc_api.guidance_record_attachments
    USING (record_id IN (SELECT id FROM alc_api.guidance_records));

-- テナントごとの許可メール (招待)
CREATE TABLE IF NOT EXISTS alc_api.tenant_allowed_emails (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id) ON DELETE CASCADE,
  email TEXT NOT NULL,
  role TEXT NOT NULL DEFAULT 'admin' CHECK (role IN ('admin', 'viewer')),
  created_at TIMESTAMPTZ DEFAULT NOW(),
  UNIQUE(email)
);

-- RLS: テナントスコープ
ALTER TABLE alc_api.tenant_allowed_emails ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON alc_api.tenant_allowed_emails
  FOR ALL TO alc_api_app
  USING (tenant_id = NULLIF(current_setting('app.current_tenant_id', true), '')::UUID);

-- aoi を即座に招待登録 (テナントが存在する場合のみ)
INSERT INTO alc_api.tenant_allowed_emails (tenant_id, email, role)
SELECT '536859de-d43e-4932-9d16-f60cac8fa426', 'aoi@ohishiunyusouko.com', 'admin'
WHERE EXISTS (SELECT 1 FROM alc_api.tenants WHERE id = '536859de-d43e-4932-9d16-f60cac8fa426')
ON CONFLICT (email) DO NOTHING;

-- テナントに email_domain カラム追加 (同ドメインの将来のユーザー用)
ALTER TABLE alc_api.tenants ADD COLUMN IF NOT EXISTS email_domain TEXT;
UPDATE alc_api.tenants SET email_domain = 'ohishiunyusouko.com'
  WHERE id = '536859de-d43e-4932-9d16-f60cac8fa426' AND email_domain IS NULL;

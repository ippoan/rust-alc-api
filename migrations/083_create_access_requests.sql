-- アクセスリクエスト (テナント参加申請)
CREATE TABLE IF NOT EXISTS alc_api.access_requests (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
  user_id UUID NOT NULL REFERENCES alc_api.users(id),
  status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'declined')),
  role TEXT NOT NULL DEFAULT 'viewer',
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- RLS 有効化
ALTER TABLE alc_api.access_requests ENABLE ROW LEVEL SECURITY;

-- テナント管理者が自テナントのリクエストを参照可能
CREATE POLICY access_requests_select ON alc_api.access_requests
  FOR SELECT USING (tenant_id = alc_api.current_tenant_id());

-- 認証済みユーザーがリクエストを作成可能
CREATE POLICY access_requests_insert ON alc_api.access_requests
  FOR INSERT WITH CHECK (user_id = alc_api.current_user_id());

-- テナント管理者がステータスを更新可能
CREATE POLICY access_requests_update ON alc_api.access_requests
  FOR UPDATE USING (tenant_id = alc_api.current_tenant_id());

-- GRANT
GRANT SELECT, INSERT, UPDATE ON alc_api.access_requests TO alc_api_app;

-- tenants テーブルに slug カラムが存在しない場合のみ追加 (既に存在する場合はスキップ)
-- slug はテナント参加ページで使用
DO $$ BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.columns
    WHERE table_schema = 'alc_api' AND table_name = 'tenants' AND column_name = 'slug'
  ) THEN
    ALTER TABLE alc_api.tenants ADD COLUMN slug TEXT UNIQUE;
  END IF;
END $$;

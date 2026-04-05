-- パスワード認証のための password_hash カラム追加
ALTER TABLE alc_api.users ADD COLUMN IF NOT EXISTS password_hash TEXT;

-- パスワードログインに使う username カラム追加 (email と別にユーザー名でもログイン可能にする)
ALTER TABLE alc_api.users ADD COLUMN IF NOT EXISTS username TEXT;

-- username はテナント内でユニーク
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_tenant_username
  ON alc_api.users (tenant_id, username)
  WHERE username IS NOT NULL;

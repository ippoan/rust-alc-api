-- Phase 1: テナント/ユーザー統一モデル
-- tenants に slug 追加、users に LINE WORKS OAuth 対応

-- tenants テーブルに slug カラム追加
ALTER TABLE alc_api.tenants ADD COLUMN IF NOT EXISTS slug TEXT UNIQUE;

-- users テーブルに lineworks_id カラム追加
ALTER TABLE alc_api.users ADD COLUMN IF NOT EXISTS lineworks_id TEXT UNIQUE;

-- google_sub を nullable に変更（LINE WORKS ユーザーは google_sub を持たない）
ALTER TABLE alc_api.users ALTER COLUMN google_sub DROP NOT NULL;

-- google_sub の UNIQUE 制約を再作成（NULL を許容する形）
-- 既存の制約を確認して削除 → 再作成
ALTER TABLE alc_api.users DROP CONSTRAINT IF EXISTS users_google_sub_key;
CREATE UNIQUE INDEX IF NOT EXISTS users_google_sub_unique ON alc_api.users (google_sub) WHERE google_sub IS NOT NULL;

-- google_sub OR lineworks_id のどちらかは必須
ALTER TABLE alc_api.users ADD CONSTRAINT user_has_provider
    CHECK (google_sub IS NOT NULL OR lineworks_id IS NOT NULL);

-- rust-logi の Default Organization をテナントとして登録（carins 用）
INSERT INTO alc_api.tenants (id, name, slug, created_at)
VALUES ('00000000-0000-0000-0000-000000000001', 'Default Organization', 'default', NOW())
ON CONFLICT (id) DO UPDATE SET slug = 'default';

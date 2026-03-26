-- rust-alc-api ローカルテスト用 DB 初期化
-- 本番 Supabase と同等のスキーマ・ロール構成を再現

-- alc_api スキーマ
CREATE SCHEMA IF NOT EXISTS alc_api;

-- アプリケーションロール (NOBYPASSRLS = RLS が有効)
CREATE ROLE alc_api_app NOLOGIN NOBYPASSRLS;
GRANT USAGE ON SCHEMA alc_api TO alc_api_app;

-- search_path をデフォルトで alc_api に設定
-- (migration 001-025 の非修飾テーブル名が alc_api スキーマに作成されるように)
ALTER DATABASE postgres SET search_path TO alc_api, public;

-- Supabase 互換ロール
CREATE ROLE anon NOLOGIN;
CREATE ROLE authenticated NOLOGIN;
CREATE ROLE service_role NOLOGIN;
GRANT USAGE ON SCHEMA public TO anon, authenticated, service_role;
GRANT USAGE ON SCHEMA alc_api TO anon, authenticated, service_role;

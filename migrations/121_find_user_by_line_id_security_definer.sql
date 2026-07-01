-- LINE ログイン中の users 逆引きを SECURITY DEFINER でバイパス (Refs #434)
--
-- find_recipient_by_line_user_id (migration 076 / 119) と対称の措置。
-- `find_user_by_line_user_id` は従来 repo 層が `SELECT * FROM users WHERE
-- line_user_id = $1` を **直接** 実行していた。この経路は LINE ログイン中
-- (tenant 未確定 = pre-auth) の全テナント横断逆引きだが、実行ロール
-- `alc_api_app` (NOBYPASSRLS, テーブル非所有者) には users の RLS ポリシー
--   USING (tenant_id = current_setting('app.current_tenant_id')::UUID)   -- migration 003, missing_ok 無し
-- が適用される。internal router は tenant context をセットしないため、プール
-- 接続に残る前リクエストの GUC 次第で:
--   - GUC 未設定 / '' → current_setting(...)::UUID が例外 → 500
--   - 別テナントの GUC → 既存 LINE ユーザーを未検出 → 新規扱い → create で
--     line_user_id UNIQUE 衝突 → 500
-- と非決定的に壊れる (recipient 側は 076 で SECURITY DEFINER 化済みだが user 側は
-- 未対応だった非対称)。
--
-- 根治: recipient と同じく SECURITY DEFINER 関数で RLS をバイパスして全テナント
-- 横断で 1 件返す。所有者として実行されるため RLS を回避できる (= 認証前アクセスの
-- 本来の意図)。通常の tenant スコープ CRUD は alc_api_app に RLS が効いたまま。
--
-- splinter 対策として SET search_path = alc_api を付与 (function_search_path_mutable
-- 回避、既存 find_recipient_by_line_user_id と同形)。
CREATE OR REPLACE FUNCTION alc_api.find_user_by_line_user_id(p_line_user_id TEXT)
RETURNS SETOF alc_api.users
LANGUAGE sql SECURITY DEFINER SET search_path = alc_api AS $$
  SELECT * FROM alc_api.users WHERE line_user_id = p_line_user_id LIMIT 1;
$$;

GRANT EXECUTE ON FUNCTION alc_api.find_user_by_line_user_id(TEXT) TO alc_api_app;

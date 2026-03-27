-- Fix: device_select_by_id USING(true) がテナント分離を無効化する問題。
-- SECURITY DEFINER 関数に置換して、認証不要エンドポイントのみ ID 指定アクセスを許可。

-- 1. 過度に許可的なポリシーを削除
DROP POLICY IF EXISTS device_select_by_id ON alc_api.devices;

-- 2. デバイスIDからテナントIDを取得（認証不要エンドポイント用）
CREATE OR REPLACE FUNCTION alc_api.lookup_device_tenant(p_device_id UUID)
RETURNS UUID
LANGUAGE sql SECURITY DEFINER SET search_path = alc_api
AS $$
    SELECT tenant_id FROM alc_api.devices WHERE id = p_device_id;
$$;
GRANT EXECUTE ON FUNCTION alc_api.lookup_device_tenant(UUID) TO alc_api_app;

-- 3. デバイス設定取得（認証不要エンドポイント用）
CREATE OR REPLACE FUNCTION alc_api.get_device_settings_by_id(p_device_id UUID)
RETURNS TABLE(
    call_enabled BOOLEAN,
    call_schedule JSONB,
    status TEXT,
    last_login_employee_id UUID,
    last_login_employee_name TEXT,
    last_login_employee_role TEXT[],
    always_on BOOLEAN
)
LANGUAGE sql SECURITY DEFINER SET search_path = alc_api
AS $$
    SELECT call_enabled, call_schedule, status,
           last_login_employee_id, last_login_employee_name,
           last_login_employee_role, always_on
    FROM alc_api.devices WHERE id = p_device_id;
$$;
GRANT EXECUTE ON FUNCTION alc_api.get_device_settings_by_id(UUID) TO alc_api_app;

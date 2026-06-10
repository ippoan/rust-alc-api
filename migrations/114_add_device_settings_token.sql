-- デバイス自身が GET /api/devices/settings/{device_id} を読むための bearer token (Refs #388)。
--
-- device_id のみで call_schedule / last_login_employee_* が取れる公開エンドポイントに
-- device 保有 secret を導入する。既存行は NULL のまま = handler は移行期互換
-- (X-Device-Token ヘッダが来た時だけ検証し、NULL 端末は従来動作)。
-- 新規承認 (approve / approve-by-code / claim url) 時に backend が発行する。
ALTER TABLE alc_api.devices ADD COLUMN settings_token UUID;

-- SECURITY DEFINER 関数 (063 で導入) の戻り値に settings_token を追加する。
-- RETURNS TABLE のカラム追加は CREATE OR REPLACE では出来ないため DROP → CREATE。
DROP FUNCTION IF EXISTS alc_api.get_device_settings_by_id(UUID);
CREATE FUNCTION alc_api.get_device_settings_by_id(p_device_id UUID)
RETURNS TABLE(
    call_enabled BOOLEAN,
    call_schedule JSONB,
    status TEXT,
    last_login_employee_id UUID,
    last_login_employee_name TEXT,
    last_login_employee_role TEXT[],
    always_on BOOLEAN,
    settings_token UUID
)
LANGUAGE sql SECURITY DEFINER SET search_path = alc_api
AS $$
    SELECT call_enabled, call_schedule, status,
           last_login_employee_id, last_login_employee_name,
           last_login_employee_role, always_on, settings_token
    FROM alc_api.devices WHERE id = p_device_id;
$$;
GRANT EXECUTE ON FUNCTION alc_api.get_device_settings_by_id(UUID) TO alc_api_app;

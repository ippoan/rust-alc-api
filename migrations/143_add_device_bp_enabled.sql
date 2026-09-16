-- 血圧計 (Omron) を使うかどうかの端末設定 (Refs ippoan/alc-app-s3#135)。
--
-- これまで端末の NVS にしか無く、端末入替・初期化で消えていた。サーバを正本にする。
-- 既定は false (既存端末の挙動を変えないため)。038 の call_enabled / 050 の
-- always_on と同じ形。
ALTER TABLE alc_api.devices ADD COLUMN bp_enabled BOOLEAN NOT NULL DEFAULT false;

-- get_device_settings_by_id (063 で導入、114 で settings_token 追加) の戻り値に
-- bp_enabled を追加する。列を足しただけではこの SECURITY DEFINER 関数の列挙に
-- 乗らず GET /api/devices/settings/{device_id} に出てこないため、関数側も直す。
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
    settings_token UUID,
    bp_enabled BOOLEAN
)
LANGUAGE sql SECURITY DEFINER SET search_path = alc_api
AS $$
    SELECT call_enabled, call_schedule, status,
           last_login_employee_id, last_login_employee_name,
           last_login_employee_role, always_on, settings_token, bp_enabled
    FROM alc_api.devices WHERE id = p_device_id;
$$;
GRANT EXECUTE ON FUNCTION alc_api.get_device_settings_by_id(UUID) TO alc_api_app;

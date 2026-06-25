-- require_tenant_or_device ミドルウェア (Refs #434) の device token 検証用関数。
--
-- `devices` テーブルは RLS 有効 + SELECT が SECURITY DEFINER 関数経由 (063) のため、
-- ミドルウェア層 (tenant context 未設定) から素の SELECT では行が見えない。
-- get_device_settings_by_id (063) と同じく SECURITY DEFINER 関数で RLS を回避し、
-- (tenant_id, settings_token, status='active') の 3 条件一致を boolean で返す。
--
-- - settings_token は approval 時にのみ発行される (114)。NULL の行は決して一致しない。
-- - status='active' で disabled / rejected / pending な device を除外する
--   (稼働中 device は INSERT 時 status='active'、enable/disable で 'active'/'disabled' を遷移)。
-- - 値 (settings_token) は引数で受け取るだけで、関数も呼び出し側も log / response に echo しない。
CREATE OR REPLACE FUNCTION alc_api.verify_device_token(p_tenant_id UUID, p_token UUID)
RETURNS BOOLEAN
LANGUAGE sql SECURITY DEFINER SET search_path = alc_api
AS $$
    SELECT EXISTS(
        SELECT 1 FROM alc_api.devices
        WHERE tenant_id = p_tenant_id
          AND settings_token = p_token
          AND status = 'active'
    );
$$;
GRANT EXECUTE ON FUNCTION alc_api.verify_device_token(UUID, UUID) TO alc_api_app;

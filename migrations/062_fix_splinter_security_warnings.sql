-- splinter セキュリティ警告の修正

-- 1. set_current_tenant: search_path を固定
CREATE OR REPLACE FUNCTION set_current_tenant(tenant_id TEXT)
RETURNS VOID AS $$
BEGIN
    PERFORM set_config('app.current_tenant_id', tenant_id, false);
END;
$$ LANGUAGE plpgsql SECURITY DEFINER SET search_path = alc_api;

-- 2. prevent_tenko_record_modification: search_path を固定
CREATE OR REPLACE FUNCTION prevent_tenko_record_modification()
RETURNS TRIGGER AS $$
BEGIN
    IF OLD.status = 'completed' THEN
        RAISE EXCEPTION 'Cannot modify completed tenko record';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql SET search_path = alc_api;

-- 3. device_registration_requests INSERT ポリシー: true → 明示的条件に変更
-- 端末側は認証不要だが、status = 'pending' の新規レコードのみ許可
DROP POLICY IF EXISTS device_reg_insert ON alc_api.device_registration_requests;
CREATE POLICY device_reg_insert ON alc_api.device_registration_requests
    FOR INSERT WITH CHECK (status = 'pending');

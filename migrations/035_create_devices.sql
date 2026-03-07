-- デバイス登録テーブル (承認済みデバイス)
CREATE TABLE alc_api.devices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    device_name TEXT NOT NULL DEFAULT '',
    device_type TEXT NOT NULL DEFAULT 'kiosk' CHECK (device_type IN ('kiosk', 'android')),
    phone_number TEXT,
    user_id UUID REFERENCES alc_api.users(id),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'disabled')),
    approved_by UUID REFERENCES alc_api.users(id),
    approved_at TIMESTAMPTZ,
    last_seen_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_devices_tenant ON alc_api.devices(tenant_id);
CREATE INDEX idx_devices_user ON alc_api.devices(tenant_id, user_id) WHERE user_id IS NOT NULL;

ALTER TABLE alc_api.devices ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation_devices ON alc_api.devices
    USING (tenant_id = current_setting('app.current_tenant_id')::UUID);

-- デバイス登録リクエスト (承認待ち / ポーリング用)
CREATE TABLE alc_api.device_registration_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    registration_code TEXT NOT NULL UNIQUE,
    flow_type TEXT NOT NULL CHECK (flow_type IN ('qr_temp', 'qr_permanent', 'url')),
    tenant_id UUID REFERENCES alc_api.tenants(id),
    phone_number TEXT,
    device_name TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'rejected', 'expired')),
    device_id UUID REFERENCES alc_api.devices(id),
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_device_reg_code ON alc_api.device_registration_requests(registration_code) WHERE status = 'pending';
CREATE INDEX idx_device_reg_tenant ON alc_api.device_registration_requests(tenant_id) WHERE status = 'pending';

ALTER TABLE alc_api.device_registration_requests ENABLE ROW LEVEL SECURITY;

-- ポーリング用に SELECT は公開 (registration_code でフィルタされる)
CREATE POLICY device_reg_select ON alc_api.device_registration_requests
    FOR SELECT USING (true);
-- INSERT は公開 (QR一時フローで認証なしで作成)
CREATE POLICY device_reg_insert ON alc_api.device_registration_requests
    FOR INSERT WITH CHECK (true);
-- UPDATE はテナントスコープ or tenant_id NULL (QR一時: 承認時にセット)
CREATE POLICY device_reg_update ON alc_api.device_registration_requests
    FOR UPDATE USING (
        tenant_id IS NULL
        OR tenant_id = current_setting('app.current_tenant_id')::UUID
    );
-- DELETE はテナントスコープ
CREATE POLICY device_reg_delete ON alc_api.device_registration_requests
    FOR DELETE USING (tenant_id = current_setting('app.current_tenant_id')::UUID);

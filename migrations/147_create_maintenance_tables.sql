-- 車両整備記録 (maintenance) の土台テーブル (Refs ippoan/rust-alc-api#651)
--
-- 遠隔点呼の国交省要件 2-四-ト「運行に使用する事業用自動車の整備状況」を満たすための
-- 車両本体の整備記録。既存の equipment_failures (alc-tenko、キオスク端末・顔認証・
-- ALC センサー等 alc-app 自体の機器故障) や alc-trouble (ticket と車両の FK なし)
-- とは別物なので流用しない。
--
-- 車両 identity の正本は maintenance_vehicles。電子車検証 (car_inspection) は
-- 従属する任意のリンクで、必須にしない — car_inspection の UNIQUE は
-- (tenant_id, "ElectCertMgNo", "GrantdateE","GrantdateY","GrantdateM","GrantdateD")
-- (車検証 1 枚ごとの履歴表、048:232) で (tenant_id, "CarId") は index のみ (048:236)
-- のため FK の参照先にできず、かつ car_inspection に行があるのは一部テナントだけ。

-- 車両マスタ
CREATE TABLE alc_api.maintenance_vehicles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    registration_number TEXT NOT NULL,
    display_name TEXT,
    car_id TEXT,
    carins_linked_at TIMESTAMPTZ,
    note TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

-- 2 台が同じ車検証 (CarId) を主張するのを防ぐ。car_id が NULL (未紐づけ) の行は
-- 何行あっても衝突しない (partial unique)。
CREATE UNIQUE INDEX idx_maintenance_vehicles_tenant_car_id
    ON alc_api.maintenance_vehicles(tenant_id, car_id) WHERE car_id IS NOT NULL;

-- 自動車登録番号は移転で再割当てされるため UNIQUE にしない (通常 index)。
CREATE INDEX idx_maintenance_vehicles_tenant_reg_no
    ON alc_api.maintenance_vehicles(tenant_id, registration_number);

CREATE INDEX idx_maintenance_vehicles_active
    ON alc_api.maintenance_vehicles(tenant_id) WHERE deleted_at IS NULL;

-- 整備カテゴリ (テナントごとに追加可能。migrations/086 の trouble_categories と
-- 同じ形にする — 後続タスクが共通の generic master repo から使うため)
CREATE TABLE alc_api.maintenance_categories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    name TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(tenant_id, name)
);

-- 整備記録本体 (このタスクではテーブルのみ。handler は後続タスクの担当)
CREATE TABLE alc_api.maintenance_records (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    vehicle_id UUID NOT NULL REFERENCES alc_api.maintenance_vehicles(id),
    category_id UUID NOT NULL REFERENCES alc_api.maintenance_categories(id),
    performed_on DATE NOT NULL,
    odometer_km INTEGER,
    vendor TEXT,
    description TEXT,
    cost NUMERIC(12,2),
    next_due_on DATE,
    created_by UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

CREATE INDEX idx_maintenance_records_tenant_vehicle_performed
    ON alc_api.maintenance_records(tenant_id, vehicle_id, performed_on DESC);

-- 整備記録の添付ファイル (crates/alc-trouble/src/models.rs の TroubleFile と
-- 同じ列構成。このタスクでは handler は作らない)
CREATE TABLE alc_api.maintenance_files (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    record_id UUID NOT NULL REFERENCES alc_api.maintenance_records(id) ON DELETE CASCADE,
    filename TEXT NOT NULL,
    content_type TEXT NOT NULL DEFAULT 'application/octet-stream',
    size_bytes BIGINT NOT NULL DEFAULT 0,
    storage_key TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

CREATE INDEX idx_maintenance_files_record ON alc_api.maintenance_files(record_id);

-- RLS
ALTER TABLE alc_api.maintenance_vehicles ENABLE ROW LEVEL SECURITY;
ALTER TABLE alc_api.maintenance_categories ENABLE ROW LEVEL SECURITY;
ALTER TABLE alc_api.maintenance_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE alc_api.maintenance_files ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON alc_api.maintenance_vehicles
    FOR ALL USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

CREATE POLICY tenant_isolation ON alc_api.maintenance_categories
    FOR ALL USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

CREATE POLICY tenant_isolation ON alc_api.maintenance_records
    FOR ALL USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

CREATE POLICY tenant_isolation ON alc_api.maintenance_files
    FOR ALL USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

-- Grants (GRANT ON ALL TABLES は既存表のスナップショットなので新表に効かない。
-- 表を足したこの migration が自分で書く)
GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.maintenance_vehicles TO alc_api_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.maintenance_categories TO alc_api_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.maintenance_records TO alc_api_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.maintenance_files TO alc_api_app;

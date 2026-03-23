-- 054: daiun-salary から労働時間データ + CSV取込機能を移行
-- driver_id は employees(id) を参照（drivers テーブルは作らない）
-- employees.driver_cd で digitacho CSV の運転者コードと紐付け

-- employees に driver_cd カラム追加
ALTER TABLE alc_api.employees ADD COLUMN IF NOT EXISTS driver_cd TEXT;

-- 営業所マスタ
CREATE TABLE IF NOT EXISTS alc_api.dtako_offices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    office_cd TEXT NOT NULL,
    office_name TEXT NOT NULL,
    UNIQUE(tenant_id, office_cd)
);
CREATE INDEX IF NOT EXISTS idx_dtako_offices_tenant ON alc_api.dtako_offices(tenant_id);
ALTER TABLE alc_api.dtako_offices ENABLE ROW LEVEL SECURITY;
CREATE POLICY dtako_offices_tenant ON alc_api.dtako_offices
    USING (tenant_id = current_setting('app.current_tenant_id')::UUID);

-- 車両マスタ
CREATE TABLE IF NOT EXISTS alc_api.dtako_vehicles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    vehicle_cd TEXT NOT NULL,
    vehicle_name TEXT NOT NULL,
    UNIQUE(tenant_id, vehicle_cd)
);
CREATE INDEX IF NOT EXISTS idx_dtako_vehicles_tenant ON alc_api.dtako_vehicles(tenant_id);
ALTER TABLE alc_api.dtako_vehicles ENABLE ROW LEVEL SECURITY;
CREATE POLICY dtako_vehicles_tenant ON alc_api.dtako_vehicles
    USING (tenant_id = current_setting('app.current_tenant_id')::UUID);

-- イベント分類マスタ
CREATE TABLE IF NOT EXISTS alc_api.dtako_event_classifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    event_cd TEXT NOT NULL,
    event_name TEXT NOT NULL,
    classification TEXT NOT NULL,  -- 'drive', 'cargo', 'rest_split', 'break', 'ignore'
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(tenant_id, event_cd)
);
CREATE INDEX IF NOT EXISTS idx_dtako_event_cls_tenant ON alc_api.dtako_event_classifications(tenant_id);
ALTER TABLE alc_api.dtako_event_classifications ENABLE ROW LEVEL SECURITY;
CREATE POLICY dtako_event_cls_tenant ON alc_api.dtako_event_classifications
    USING (tenant_id = current_setting('app.current_tenant_id')::UUID);

-- 運行データ (KUDGURI CSV)
CREATE TABLE IF NOT EXISTS alc_api.dtako_operations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    unko_no TEXT NOT NULL,
    crew_role INTEGER NOT NULL DEFAULT 0,
    reading_date DATE NOT NULL,
    operation_date DATE,
    office_id UUID REFERENCES alc_api.dtako_offices(id),
    vehicle_id UUID REFERENCES alc_api.dtako_vehicles(id),
    driver_id UUID REFERENCES alc_api.employees(id),
    departure_at TIMESTAMPTZ,
    return_at TIMESTAMPTZ,
    garage_out_at TIMESTAMPTZ,
    garage_in_at TIMESTAMPTZ,
    meter_start DOUBLE PRECISION,
    meter_end DOUBLE PRECISION,
    total_distance DOUBLE PRECISION,
    drive_time_general INTEGER,
    drive_time_highway INTEGER,
    drive_time_bypass INTEGER,
    safety_score DOUBLE PRECISION,
    economy_score DOUBLE PRECISION,
    total_score DOUBLE PRECISION,
    raw_data JSONB NOT NULL DEFAULT '{}',
    r2_key_prefix TEXT,
    uploaded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    has_kudgivt BOOLEAN NOT NULL DEFAULT FALSE,
    -- operation-level aggregates
    op_drive_minutes INTEGER,
    op_cargo_minutes INTEGER,
    op_break_minutes INTEGER,
    op_restraint_minutes INTEGER,
    op_late_night_minutes INTEGER,
    op_overlap_drive_minutes INTEGER NOT NULL DEFAULT 0,
    op_overlap_cargo_minutes INTEGER NOT NULL DEFAULT 0,
    op_overlap_break_minutes INTEGER NOT NULL DEFAULT 0,
    op_overlap_restraint_minutes INTEGER NOT NULL DEFAULT 0,
    op_ot_late_night_minutes INTEGER NOT NULL DEFAULT 0,
    UNIQUE(tenant_id, unko_no, crew_role)
);
CREATE INDEX IF NOT EXISTS idx_dtako_ops_tenant ON alc_api.dtako_operations(tenant_id);
CREATE INDEX IF NOT EXISTS idx_dtako_ops_reading_date ON alc_api.dtako_operations(tenant_id, reading_date);
CREATE INDEX IF NOT EXISTS idx_dtako_ops_driver ON alc_api.dtako_operations(driver_id);
CREATE INDEX IF NOT EXISTS idx_dtako_ops_vehicle ON alc_api.dtako_operations(vehicle_id);
ALTER TABLE alc_api.dtako_operations ENABLE ROW LEVEL SECURITY;
CREATE POLICY dtako_ops_tenant ON alc_api.dtako_operations
    USING (tenant_id = current_setting('app.current_tenant_id')::UUID);

-- 日別労働時間
CREATE TABLE IF NOT EXISTS alc_api.dtako_daily_work_hours (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    driver_id UUID NOT NULL REFERENCES alc_api.employees(id),
    work_date DATE NOT NULL,
    start_time TIME NOT NULL DEFAULT '00:00:00',
    total_work_minutes INTEGER,
    total_drive_minutes INTEGER,
    total_rest_minutes INTEGER,
    late_night_minutes INTEGER NOT NULL DEFAULT 0,
    drive_minutes INTEGER NOT NULL DEFAULT 0,
    cargo_minutes INTEGER NOT NULL DEFAULT 0,
    overlap_drive_minutes INTEGER NOT NULL DEFAULT 0,
    overlap_cargo_minutes INTEGER NOT NULL DEFAULT 0,
    overlap_break_minutes INTEGER NOT NULL DEFAULT 0,
    overlap_restraint_minutes INTEGER NOT NULL DEFAULT 0,
    ot_late_night_minutes INTEGER NOT NULL DEFAULT 0,
    total_distance DOUBLE PRECISION,
    operation_count INTEGER NOT NULL DEFAULT 0,
    unko_nos TEXT[],
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(tenant_id, driver_id, work_date, start_time)
);
CREATE INDEX IF NOT EXISTS idx_dtako_dwh_tenant ON alc_api.dtako_daily_work_hours(tenant_id);
CREATE INDEX IF NOT EXISTS idx_dtako_dwh_driver_date ON alc_api.dtako_daily_work_hours(driver_id, work_date);
ALTER TABLE alc_api.dtako_daily_work_hours ENABLE ROW LEVEL SECURITY;
CREATE POLICY dtako_dwh_tenant ON alc_api.dtako_daily_work_hours
    USING (tenant_id = current_setting('app.current_tenant_id')::UUID);

-- 日別ワークセグメント
CREATE TABLE IF NOT EXISTS alc_api.dtako_daily_work_segments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    driver_id UUID NOT NULL REFERENCES alc_api.employees(id),
    work_date DATE NOT NULL,
    unko_no TEXT NOT NULL,
    segment_index INTEGER NOT NULL DEFAULT 0,
    start_at TIMESTAMPTZ NOT NULL,
    end_at TIMESTAMPTZ NOT NULL,
    work_minutes INTEGER NOT NULL,
    labor_minutes INTEGER NOT NULL DEFAULT 0,
    late_night_minutes INTEGER NOT NULL DEFAULT 0,
    drive_minutes INTEGER NOT NULL DEFAULT 0,
    cargo_minutes INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_dtako_dws_driver_date ON alc_api.dtako_daily_work_segments(tenant_id, driver_id, work_date);
ALTER TABLE alc_api.dtako_daily_work_segments ENABLE ROW LEVEL SECURITY;
CREATE POLICY dtako_dws_tenant ON alc_api.dtako_daily_work_segments
    USING (tenant_id = current_setting('app.current_tenant_id')::UUID);

-- アップロード履歴
CREATE TABLE IF NOT EXISTS alc_api.dtako_upload_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    uploaded_by UUID REFERENCES alc_api.users(id),
    filename TEXT NOT NULL,
    operations_count INTEGER NOT NULL DEFAULT 0,
    r2_zip_key TEXT,
    status TEXT NOT NULL DEFAULT 'processing',
    error_message TEXT,
    operation_year INTEGER,
    operation_month INTEGER,
    csv_split_done BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_dtako_upload_tenant ON alc_api.dtako_upload_history(tenant_id);
CREATE INDEX IF NOT EXISTS idx_dtako_upload_month ON alc_api.dtako_upload_history(tenant_id, operation_year, operation_month);
ALTER TABLE alc_api.dtako_upload_history ENABLE ROW LEVEL SECURITY;
CREATE POLICY dtako_upload_tenant ON alc_api.dtako_upload_history
    USING (tenant_id = current_setting('app.current_tenant_id')::UUID);

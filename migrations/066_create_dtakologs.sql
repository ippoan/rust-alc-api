-- dtakologs テーブル（rust-logi から移行）
-- リアルタイム車両GPS/運行ログ

CREATE TABLE IF NOT EXISTS alc_api.dtakologs (
    -- 複合主キー
    data_date_time TEXT NOT NULL,
    vehicle_cd INTEGER NOT NULL,
    tenant_id UUID NOT NULL,

    -- 必須フィールド
    type TEXT NOT NULL DEFAULT '',
    all_state_font_color_index INTEGER NOT NULL DEFAULT 0,
    all_state_ryout_color TEXT NOT NULL DEFAULT 'Transparent',
    branch_cd INTEGER NOT NULL DEFAULT 0,
    branch_name TEXT NOT NULL DEFAULT '',
    current_work_cd INTEGER NOT NULL DEFAULT 0,
    data_filter_type INTEGER NOT NULL DEFAULT 0,
    disp_flag INTEGER NOT NULL DEFAULT 0,
    driver_cd INTEGER NOT NULL DEFAULT 0,
    gps_direction INTEGER NOT NULL DEFAULT 0,
    gps_enable INTEGER NOT NULL DEFAULT 0,
    gps_latitude INTEGER NOT NULL DEFAULT 0,
    gps_longitude INTEGER NOT NULL DEFAULT 0,
    gps_satellite_num INTEGER NOT NULL DEFAULT 0,
    operation_state INTEGER NOT NULL DEFAULT 0,
    recive_event_type INTEGER NOT NULL DEFAULT 0,
    recive_packet_type INTEGER NOT NULL DEFAULT 0,
    recive_work_cd INTEGER NOT NULL DEFAULT 0,
    revo INTEGER NOT NULL DEFAULT 0,
    setting_temp TEXT NOT NULL DEFAULT '',
    setting_temp1 TEXT NOT NULL DEFAULT '',
    setting_temp3 TEXT NOT NULL DEFAULT '',
    setting_temp4 TEXT NOT NULL DEFAULT '',
    speed REAL NOT NULL DEFAULT 0.0,
    sub_driver_cd INTEGER NOT NULL DEFAULT 0,
    temp_state INTEGER NOT NULL DEFAULT 0,
    vehicle_name TEXT NOT NULL DEFAULT '',

    -- オプショナルフィールド
    address_disp_c TEXT,
    address_disp_p TEXT,
    all_state TEXT,
    all_state_ex TEXT,
    all_state_font_color TEXT,
    comu_date_time TEXT,
    current_work_name TEXT,
    driver_name TEXT,
    event_val TEXT,
    gps_lati_and_long TEXT,
    odometer TEXT,
    recive_type_color_name TEXT,
    recive_type_name TEXT,
    start_work_date_time TEXT,
    state TEXT,
    state1 TEXT,
    state2 TEXT,
    state3 TEXT,
    state_flag TEXT,
    temp1 TEXT,
    temp2 TEXT,
    temp3 TEXT,
    temp4 TEXT,
    vehicle_icon_color TEXT,
    vehicle_icon_label_for_datetime TEXT,
    vehicle_icon_label_for_driver TEXT,
    vehicle_icon_label_for_vehicle TEXT,

    -- タイムスタンプ
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- 複合主キー
    PRIMARY KEY (tenant_id, data_date_time, vehicle_cd)
);

-- インデックス
CREATE INDEX IF NOT EXISTS idx_dtakologs_tenant_id ON alc_api.dtakologs(tenant_id);
CREATE INDEX IF NOT EXISTS idx_dtakologs_vehicle_cd ON alc_api.dtakologs(tenant_id, vehicle_cd);
CREATE INDEX IF NOT EXISTS idx_dtakologs_data_date_time ON alc_api.dtakologs(tenant_id, data_date_time DESC);
CREATE INDEX IF NOT EXISTS idx_dtakologs_address_disp_p ON alc_api.dtakologs(tenant_id, address_disp_p) WHERE address_disp_p IS NOT NULL;

-- RLS
ALTER TABLE alc_api.dtakologs ENABLE ROW LEVEL SECURITY;
ALTER TABLE alc_api.dtakologs FORCE ROW LEVEL SECURITY;

DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_policies WHERE tablename = 'dtakologs' AND schemaname = 'alc_api' AND policyname = 'tenant_isolation') THEN
        CREATE POLICY tenant_isolation ON alc_api.dtakologs
            USING (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ))
            WITH CHECK (tenant_id = COALESCE(
                NULLIF(current_setting('app.current_tenant_id', true), '')::UUID,
                NULLIF(current_setting('app.current_organization_id', true), '')::UUID
            ));
    END IF;
END $$;

GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.dtakologs TO alc_api_app;

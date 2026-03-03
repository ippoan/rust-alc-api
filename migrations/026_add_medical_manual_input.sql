-- 体温・血圧の手動入力フラグを追加
-- true: 手動入力, false/NULL: BLE機器計測

ALTER TABLE alc_api.tenko_sessions
    ADD COLUMN IF NOT EXISTS medical_manual_input BOOLEAN;

ALTER TABLE alc_api.measurements
    ADD COLUMN IF NOT EXISTS medical_manual_input BOOLEAN;

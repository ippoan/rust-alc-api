-- time_punches に device_id カラム追加（どのデバイスから打刻したか記録）
ALTER TABLE alc_api.time_punches
    ADD COLUMN device_id UUID REFERENCES alc_api.devices(id);

CREATE INDEX idx_time_punches_device ON alc_api.time_punches(device_id) WHERE device_id IS NOT NULL;

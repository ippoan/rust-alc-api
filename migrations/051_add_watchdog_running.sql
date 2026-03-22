-- 端末からのWatchdogService稼働状態の報告を保存
ALTER TABLE alc_api.devices ADD COLUMN watchdog_running BOOLEAN;

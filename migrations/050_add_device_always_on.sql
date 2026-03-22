-- デバイスの常時起動設定 (管理画面から ON/OFF 制御)
ALTER TABLE alc_api.devices ADD COLUMN always_on BOOLEAN NOT NULL DEFAULT true;

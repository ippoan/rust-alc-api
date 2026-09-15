-- 通常点呼の記録に電子車検証の管理番号・車両 ID と、carins の車検期限を残す
-- Refs ippoan/alc-app-s3#110, ippoan/alc-app-s3#135
--
-- 運行者端末が電子車検証をタップして読んだ番号を測定の保存に載せ、保存と同じ
-- transaction で car_inspection を照合して期限を写す (照合の SQL は alc-core の
-- repo/car_inspections.rs の 1 か所)。
--
-- 列の意味:
-- * carins_cert_no / carins_vehicle_id — 端末から受け取った電子車検証の管理番号 / 車両 ID
-- * carins_expires_on — 照合できた car_inspection の 2 次元コードの有効期限
-- * carins_matched_by — NULL = 番号を受け取っていない (または照合に失敗した)、
--   'cert_no' / 'car_id' = どちらの番号で一致したか、'none' = 受け取ったが carins に無い
--
-- 既存行は NULL のまま (UPDATE しない)。FK は張らない (car_inspection は取り込みで
-- 行が差し替わるため)。tenko_records は不変テーブルなので列を足さず、record_data に
-- 載った値を CSV が読む。
--
-- 切り替えの間: 旧版の `RETURNING *` は sqlx の FromRow が余分な列を無視するので壊れない。
ALTER TABLE alc_api.tenko_sessions
    ADD COLUMN IF NOT EXISTS carins_cert_no TEXT,
    ADD COLUMN IF NOT EXISTS carins_vehicle_id TEXT,
    ADD COLUMN IF NOT EXISTS carins_expires_on DATE,
    ADD COLUMN IF NOT EXISTS carins_matched_by TEXT
        CHECK (carins_matched_by IN ('cert_no', 'car_id', 'none'));

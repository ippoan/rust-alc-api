-- 057: 携行品に車両分類カラムを追加
-- NULL = 全車両共通、値あり = 特定の車両分類のみ

ALTER TABLE alc_api.carrying_items ADD COLUMN IF NOT EXISTS vehicle_category TEXT;

COMMENT ON COLUMN alc_api.carrying_items.vehicle_category IS '車両分類 (NULL=全車両共通)';

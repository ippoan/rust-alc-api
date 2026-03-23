-- 060: 携行品の車両分類条件テーブル (正規化)
-- 同カテゴリ内 = OR、異カテゴリ間 = AND
-- vehicle_category TEXT カラムは廃止

CREATE TABLE IF NOT EXISTS alc_api.carrying_item_vehicle_conditions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    carrying_item_id UUID NOT NULL REFERENCES alc_api.carrying_items(id) ON DELETE CASCADE,
    category TEXT NOT NULL CHECK (category IN ('car_kind', 'use', 'car_shape', 'private_business')),
    value TEXT NOT NULL,
    UNIQUE(carrying_item_id, category, value)
);

CREATE INDEX idx_civc_item ON alc_api.carrying_item_vehicle_conditions(carrying_item_id);
CREATE INDEX idx_civc_category_value ON alc_api.carrying_item_vehicle_conditions(category, value);

ALTER TABLE alc_api.carrying_item_vehicle_conditions ENABLE ROW LEVEL SECURITY;
CREATE POLICY civc_tenant ON alc_api.carrying_item_vehicle_conditions
    USING (carrying_item_id IN (SELECT id FROM alc_api.carrying_items));

-- 旧カラム削除
ALTER TABLE alc_api.carrying_items DROP COLUMN IF EXISTS vehicle_category;

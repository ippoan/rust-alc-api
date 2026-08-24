-- 経過記録 (trouble_tasks) の並び替えが効かない不具合の是正 (Refs ippoan/nuxt-trouble#240)。
--
-- sort_order は 089 で `NOT NULL DEFAULT 0` として作られ、作成 API も既定 0 を
-- 入れていたため、既存行は 1 チケット内で全て 0 になっている。一覧は
-- `ORDER BY sort_order, created_at` なので、隣接行の sort_order を交換する
-- 並び替え操作が「0 と 0 の交換」= 無変化になっていた。
--
-- ここでは一覧と同じ並び (sort_order, created_at) のまま 0 起点で連番に振り直す。
-- 現在の表示順を保つ変換なので、既に手で並び替え済みのチケットも順序は変わらない。
-- ハードコードした id / 値は使わず、既存行だけを対象に相対的に採番する。

UPDATE alc_api.trouble_tasks t
SET sort_order = s.rn - 1
FROM (
    SELECT id,
           row_number() OVER (
               PARTITION BY ticket_id
               ORDER BY sort_order, created_at, id
           ) AS rn
    FROM alc_api.trouble_tasks
) s
WHERE t.id = s.id
  AND t.sort_order <> s.rn - 1;

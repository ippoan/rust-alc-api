-- 通常点呼の記録に始業 / 終業の種別を付ける
-- Refs ippoan/alc-app-s3#135
--
-- migration 140 は通常点呼を「業務前 / 業務後で分けず normal 1 種 (オーナー判断)」と
-- していた。2026-09 のオーナー判断でこれを改め、運行者端末で始業 / 終業を選ぶと
-- tenko_type = pre_operation / post_operation で記録する (選ばなければ normal のまま)。
--
-- 種別だけでは自動点呼・遠隔点呼の session と見分けられなくなるため、
-- tenko_sessions にも tenko_records と同じ tenko_method を持たせ、冪等の部分 unique を
-- 「通常点呼の流れで作った session」に掛け直す。
--
-- 切り替えの間 (この migration の適用後、新しい版へ切り替わるまで) について:
-- * 旧版は tenko_method を書かずに normal の session を作るので、tenko_method は
--   DEFAULT の '自動点呼' のまま残りうる。tenko_type = 'normal' なので下の部分 unique には
--   掛かる。tenko_method を読む経路は今のところ無い
-- * 旧版で記録された測定は、後から新しい版へ pre_operation で再送されても normal のまま
--   残る (部分 unique が弾くので 1 組は保たれる)

-- 1. 点呼方法の列。tenko_records の DEFAULT (migration 015) と同じ値にそろえる。
--    既存の自動点呼・遠隔点呼の session は DEFAULT のままで実態と一致する。
ALTER TABLE alc_api.tenko_sessions
    ADD COLUMN IF NOT EXISTS tenko_method TEXT NOT NULL DEFAULT '自動点呼'
    CHECK (tenko_method IN ('自動点呼', '通常点呼'));

-- 2. 既存の通常点呼の session を '通常点呼' にそろえる。件数は deploy ログへ出す。
DO $$
DECLARE
    updated_sessions BIGINT;
BEGIN
    UPDATE alc_api.tenko_sessions SET tenko_method = '通常点呼' WHERE tenko_type = 'normal';
    GET DIAGNOSTICS updated_sessions = ROW_COUNT;

    RAISE NOTICE 'migration 141: tenko_method=通常点呼 に更新した tenko_sessions % 行',
        updated_sessions;
END
$$;

-- 3. 冪等の部分 unique を掛け直す (DROP と CREATE は同じ transaction なので index が無い瞬間は無い)。
--    述語は 140 の `tenko_type = 'normal'` の上位集合。切り替えの間に旧版が作る行
--    (normal, '自動点呼') も新しい版の行 (normal / pre / post, '通常点呼') も同じ index に
--    掛かるので、どちらの版へ再送されても 1 組に収まる。
--    `tenko_type IN (...)` へ広げないのは、自動点呼・遠隔点呼も measurement_id を UPDATE で
--    書くため (同じ測定を通常点呼と両方で使うと unique 違反になる)。
DROP INDEX IF EXISTS alc_api.uq_tenko_sessions_normal_measurement;
DROP INDEX IF EXISTS alc_api.uq_tenko_sessions_normal_employee_started;

-- 同じ測定への完了 PUT が 2 回届いても記録は 1 組
CREATE UNIQUE INDEX uq_tenko_sessions_normal_flow_measurement
    ON alc_api.tenko_sessions (measurement_id)
    WHERE tenko_type = 'normal' OR tenko_method = '通常点呼';

-- オンラインの完了 PUT が落ちてオフライン保存へ回り、測定がもう 1 件できる経路。
-- started_at = measured_at が同じなのでここで弾ける。
CREATE UNIQUE INDEX uq_tenko_sessions_normal_flow_employee_started
    ON alc_api.tenko_sessions (employee_id, started_at)
    WHERE tenko_type = 'normal' OR tenko_method = '通常点呼';

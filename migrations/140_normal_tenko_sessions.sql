-- 通常点呼 (運行者端末の測定) を点呼セッション・点呼記録に載せる
-- Refs ippoan/alc-app#238, ippoan/alc-app-s3#135
--
-- 運行者端末の「通常点呼」は measurements だけを書いていたため、運行管理者の
-- 「点呼」「点呼記録」に 1 件も出なかった。測定の保存と同じ transaction で
-- tenko_sessions / tenko_records を作れるようにするための土台を 3 つ用意する。

-- 1. 点呼執行者を空欄にできるようにする。
--    通常点呼は執行者が付かない (管理者ログイン無しの端末で完結する)。
--    tenko_sessions 側は migration 027 で既に NULL 可。records だけ NOT NULL が
--    残っていたため、執行者 NULL の session から記録を作ろうとすると必ず失敗していた。
ALTER TABLE alc_api.tenko_records ALTER COLUMN responsible_manager_name DROP NOT NULL;

-- 2. tenko_type に 'normal' を足す (migration 018 の status_check と同じ DROP → ADD 手順)。
--    業務前 / 業務後で分けず「通常」1 種にする (オーナー判断)。
ALTER TABLE alc_api.tenko_sessions DROP CONSTRAINT IF EXISTS tenko_sessions_tenko_type_check;
ALTER TABLE alc_api.tenko_sessions ADD CONSTRAINT tenko_sessions_tenko_type_check
    CHECK (tenko_type IN ('pre_operation', 'post_operation', 'normal'));

-- 3. 冪等の担保 (部分 unique)。
--    どちらも tenko_type = 'normal' の行が現時点で 0 件なので必ず張れるが、
--    張れなかったときに原因が分かるよう件数を deploy ログへ出しておく。
DO $$
DECLARE
    normal_sessions BIGINT;
BEGIN
    SELECT count(*) INTO normal_sessions
    FROM alc_api.tenko_sessions WHERE tenko_type = 'normal';

    RAISE NOTICE 'migration 140: tenko_type=normal の既存 tenko_sessions % 行 (期待値 0)',
        normal_sessions;
END
$$;

-- 同じ測定への完了 PUT が 2 回届いても記録は 1 組
CREATE UNIQUE INDEX uq_tenko_sessions_normal_measurement
    ON alc_api.tenko_sessions (measurement_id)
    WHERE tenko_type = 'normal';

-- オンラインの完了 PUT が落ちてオフライン保存へ回り、測定がもう 1 件できる経路。
-- started_at = measured_at が同じなのでここで弾ける。
CREATE UNIQUE INDEX uq_tenko_sessions_normal_employee_started
    ON alc_api.tenko_sessions (employee_id, started_at)
    WHERE tenko_type = 'normal';

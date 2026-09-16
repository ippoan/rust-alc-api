-- 自動点呼 → 遠隔点呼への切り替えを記録する
-- Refs ippoan/alc-app-s3#135
--
-- 自動点呼タブで血圧を必須にする (rust-alc-api 側の submit_medical バリデーション、
-- 別 commit) に伴い、血圧が測れないときに遠隔点呼へ切り替える逃げ道が要る
-- (画面側は ippoan/alc-app-s3#135-41)。
--
-- 141 の tenko_method CHECK は '自動点呼' / '通常点呼' の 2 値だけだったので、
-- 3 値目の '遠隔点呼' を追加する。切り替えた時刻と理由も残す。
--
-- ---------------------------------------------------------------------------
-- なぜ制約名を決め打ちせず pg_constraint から引くのか (131 と同じ理由)
-- ---------------------------------------------------------------------------
-- 141 のインライン CHECK は PostgreSQL の既定命名 (<table>_<column>_check) の
-- はずだが、本番 DB (Supabase) の pg_constraint を事前に見る手段が無く、
-- migration 履歴の外で手作業の改名・張り直しが行われていても grep では
-- 検出できない。名前を決め打ちすると、ズレていた場合に本番デプロイで
-- 初めて落ちる。DROP に IF EXISTS を付けないのも同じ理由 — 付けると
-- 名前が違ったときに DROP が黙って no-op になり、旧 CHECK が残ったまま
-- 新 CHECK が足されて '遠隔点呼' が拒否され続ける、一番気付きにくい
-- 壊れ方になる。下の DO ブロックは「tenko_method 列の CHECK がちょうど
-- 1 個」でなければ RAISE EXCEPTION で落とすので、沈黙しない性質を保つ。

DO $$
DECLARE
    v_conname TEXT;
    v_count   INT;
BEGIN
    SELECT count(*), min(c.conname)
      INTO v_count, v_conname
      FROM pg_constraint c
      JOIN pg_attribute a
        ON a.attrelid = c.conrelid
       AND a.attnum = ANY (c.conkey)
     WHERE c.conrelid = 'alc_api.tenko_sessions'::regclass
       AND c.contype = 'c'
       AND a.attname = 'tenko_method';

    IF v_count <> 1 THEN
        RAISE EXCEPTION
            'migration 144: tenko_sessions の tenko_method 列に掛かる CHECK 制約が % 個見つかりました (1 個であるはず)。'
            ' migration 履歴の外で制約が張り替えられている可能性があります。'
            ' pg_constraint を確認してから再実行してください。',
            v_count;
    END IF;

    RAISE NOTICE 'migration 144: tenko_method CHECK 制約 "%" を DROP します', v_conname;
    EXECUTE format('ALTER TABLE alc_api.tenko_sessions DROP CONSTRAINT %I', v_conname);
END
$$;

-- 張り直しは新規作成なので名前が衝突しない。既定命名に揃えておく。
ALTER TABLE alc_api.tenko_sessions
    ADD CONSTRAINT tenko_sessions_tenko_method_check
    CHECK (tenko_method IN ('自動点呼', '通常点呼', '遠隔点呼'));

-- 切り替えた時刻と理由。escalate-remote 呼び出し以外では NULL のまま
-- (interrupted_at / resumed_at + resume_reason と同じ形)。
-- 列名 escalated_to_remote_at は画面側 (#c135-41) が読む JSON キー名と揃えてある
-- (親 #p135 の決定。このタスクの PR 内で 1 度リネームしただけで、この migration が
-- 本 PR 以外の環境に出たことは無い)
ALTER TABLE alc_api.tenko_sessions
    ADD COLUMN IF NOT EXISTS escalated_to_remote_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS remote_escalation_reason TEXT;

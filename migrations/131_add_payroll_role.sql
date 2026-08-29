-- role に 'payroll' (給与閲覧) を追加する。
--
-- 広げる先は 2 つで、必ず同時に広げる:
--   - alc_api.users.role                 (003 で作成) — ログイン本体
--   - alc_api.tenant_allowed_emails.role (053 で作成) — 招待
-- 片方だけ広げると「招待は通るのにログインで CHECK 違反」というズレが出るため、
-- 1 つの migration にまとめている。
--
-- この migration は CHECK を広げるだけで、既存行の role は 1 件も書き換えない
-- (UPDATE を書かない)。この時点で 'payroll' を持つ行は 0 件なので、
-- API の挙動は変わらない。値を先に受け付けられる状態にするのが目的。
--
-- 'payroll' は「給与の実額を見てよいか」を決める独立した軸で、上流の
-- email allowlist と AND で効く。role が allowlist を上書きすることはない。
-- テナント管理 (SSO 設定 / API トークン / bot 設定 / メンバー管理 /
-- 参加リクエスト) の権限判定は従来どおり role = 'admin' のみで、
-- 'payroll' はそれらでは 403 のまま — ここでは緩めない。
--
-- 注意: employees.role (024 / 028) は運行者 / 運行管理者 / システム管理者の
-- 別テーブル・別ドメイン。同名の 'admin' が出てくるが、ここでは触らない。
--
-- ---------------------------------------------------------------------------
-- なぜ制約名を決め打ちせず pg_constraint から引くのか
-- ---------------------------------------------------------------------------
-- 003 / 053 はどちらもインライン CHECK なので、名前は PostgreSQL の既定命名
-- (<table>_<column>_check) になっているはず。004〜130 に改名・再作成も無い。
-- それでも決め打ちしないのは、**本番 DB (Supabase) の pg_constraint を
-- 事前に見る手段が無い**ため。migration 履歴の外で手作業の改名や制約の
-- 張り直しが行われていた場合、grep では検出できない。
--
-- そして、その drift を本番より前に踏む経路が無い:
--   - staging (PR ごとに deploy) は postgres sidecar の**揮発 DB** で、
--     cold start ごとに 001 から順に流し直す (staging/entrypoint.sh)。
--     つまり staging の制約名は migration が作った既定名そのもので、
--     **定義上ズレようがない** → drift の検出には使えない
--   - migration-safety-check.yml は SQL の静的検査のみで DB に当てない
--     (しかも warning only)
--   - 本番へは deploy.yml の migrate job (Cloud Run Jobs) が
--     v* タグ push でのみ実行する
-- ⇒ 名前を決め打ちすると、ズレていた場合に**本番デプロイで初めて**落ちる。
--
-- DROP に `IF EXISTS` を付けないのも同じ理由。付けると名前が違ったときに
-- DROP が黙って no-op になり、旧 CHECK が残ったまま新 CHECK が足されて
-- 'payroll' が拒否され続ける — 一番気付きにくい壊れ方になる。
-- 下の DO ブロックは「role 列の CHECK がちょうど 1 個」でなければ
-- RAISE EXCEPTION で落とすので、沈黙しない性質はそのまま保たれる。

DO $$
DECLARE
    v_table   TEXT;
    v_conname TEXT;
    v_count   INT;
BEGIN
    FOREACH v_table IN ARRAY ARRAY['alc_api.users', 'alc_api.tenant_allowed_emails']
    LOOP
        -- role 列に掛かる CHECK 制約を実際に引く。
        -- contype = 'c' は CHECK のみ (NOT NULL は attnotnull / contype = 'n' で
        -- 別枠なのでここには出てこない)。conkey に role の attnum を含むものが対象。
        SELECT count(*), min(c.conname)
          INTO v_count, v_conname
          FROM pg_constraint c
          JOIN pg_attribute a
            ON a.attrelid = c.conrelid
           AND a.attnum = ANY (c.conkey)
         WHERE c.conrelid = v_table::regclass
           AND c.contype = 'c'
           AND a.attname = 'role';

        IF v_count <> 1 THEN
            RAISE EXCEPTION
                'migration 131: % の role 列に掛かる CHECK 制約が % 個見つかりました (1 個であるはず)。'
                ' migration 履歴の外で制約が張り替えられている可能性があります。'
                ' pg_constraint を確認してから再実行してください。',
                v_table, v_count;
        END IF;

        RAISE NOTICE 'migration 131: % の role CHECK 制約 "%" を DROP します', v_table, v_conname;
        EXECUTE format('ALTER TABLE %s DROP CONSTRAINT %I', v_table::regclass, v_conname);
    END LOOP;
END
$$;

-- 張り直しは新規作成なので名前が衝突しない。既定命名に揃えておく。
ALTER TABLE alc_api.users
    ADD CONSTRAINT users_role_check
    CHECK (role IN ('admin', 'viewer', 'payroll'));

ALTER TABLE alc_api.tenant_allowed_emails
    ADD CONSTRAINT tenant_allowed_emails_role_check
    CHECK (role IN ('admin', 'viewer', 'payroll'));

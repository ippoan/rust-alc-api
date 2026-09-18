-- 乗務員 (employees) の `(tenant_id, driver_cd)` に部分一意 index を張り、二重登録の再発を止める
-- (Refs ippoan/rust-alc-api#673)
--
-- 149 (`149_employees_dedup_by_driver_cd.sql`) は**既に出来ていた重複を畳んだだけ**で、
-- 再発は止めていない。149 の冒頭コメントが「一意 index は張らない — upsert_by_code に
-- 『code 一致行を deleted_at = NULL で復活させる』3 本目の書き手があり、復活した行が同
-- driver_cd の生存行と衝突すると同期バッチのトランザクションごと 500 になる。復活経路を
-- 塞ぐのとセットで別 PR にする」と書いたとおり、**その復活経路は PR #674 で塞いである**:
--
--   * crates/alc-misc/src/repo/employees.rs — 復活 2 経路は、衝突しているときだけ
--     driver_cd を NULL にして手放し、skipped に `driver_cd_conflict` を残す
--   * crates/alc-dtako/src/repo/dtako_upload.rs — 新規 INSERT は target 無しの
--     `ON CONFLICT DO NOTHING` で、衝突しても 1 件の取り込みを 500 にしない
--
-- ⇒ 制約に違反しうる書き手はもう居ないので、ここで index を張る。
--
-- 述語を `driver_cd IS NOT NULL AND deleted_at IS NULL` にする理由:
--
--   * `deleted_at IS NULL` が無いと、**論理削除済みの行と現役の行が衝突する**。
--     この repo の削除は常に soft-delete (行は残る) なので、退職者が握ったままの
--     driver_cd を同じ乗務員CD の新しい行が取れなくなる
--   * `driver_cd IS NOT NULL` が無いと、**driver_cd 未設定の行同士が衝突する**。
--     driver_cd は dtako 取り込み経路でしか入らず、theearth 同期だけで作られた行は
--     NULL のまま何行でも並ぶ (SQL の NULL は互いに distinct なので実害は無いが、
--     述語に入れておくと index に載る行が減り、意図も明示できる)。
--     同じ形の先例: migrations/006_employee_code.sql の idx_employees_code
--
-- ★ この index が在ると migration 149 の DO ブロックはもう流せない — 勝者に driver_cd を
--    バックフィルする UPDATE が、まだ soft-delete されていない敗者と衝突するため。
--    149 は適用済みの一度きりの整理なので実害は無いが、手で流し直さないこと。

-- 1. 生存重複の事前確認。0 件でなければ index を張らずに止める。
--    **ここで畳まない**のは意図的: 149 が統合済みなので 0 件のはずで、0 でないなら
--    「149 の後に新しい重複が入った」という別の事実になる。黙って dedup を走らせると
--    149 相当のロジックが 2 本に増え、どちらが正本か分からなくなる。止めて人が見る方が安い。
DO $precheck$
DECLARE
    dup_groups INT;
    dup_detail TEXT;
BEGIN
    SELECT COUNT(*) INTO dup_groups FROM (
        SELECT tenant_id, driver_cd
          FROM alc_api.employees
         WHERE driver_cd IS NOT NULL
           AND deleted_at IS NULL
         GROUP BY tenant_id, driver_cd
        HAVING COUNT(*) > 1
    ) d;

    IF dup_groups > 0 THEN
        -- 落ちたときに CI / staging のログだけで原因が分かるよう、重複した組を列挙する
        -- (件数が読めなくなるので先頭 20 組で打ち切る)。
        SELECT string_agg(
                   format('(tenant_id=%s, driver_cd=%s) x %s 行', tenant_id, driver_cd, live_rows),
                   ', ' ORDER BY tenant_id, driver_cd)
          INTO dup_detail
          FROM (
            SELECT tenant_id, driver_cd, COUNT(*) AS live_rows
              FROM alc_api.employees
             WHERE driver_cd IS NOT NULL
               AND deleted_at IS NULL
             GROUP BY tenant_id, driver_cd
            HAVING COUNT(*) > 1
             ORDER BY tenant_id, driver_cd
             LIMIT 20
          ) d;

        RAISE EXCEPTION
            'employees の (tenant_id, driver_cd) に生存重複が % 組あるため一意 index を張れない', dup_groups
            USING DETAIL = format('重複している組 (先頭 20 組まで): %s', dup_detail),
                  HINT   = '149 の後に入った重複です。149 と同じ流儀の統合 migration を新しい連番で 1 本足してから出し直してください (149 は変更しない)。';
    END IF;
END
$precheck$;

-- 2. 部分一意 index。CONCURRENTLY は使わない (migration は 1 ファイル 1 トランザクションで走る)。
CREATE UNIQUE INDEX idx_employees_tenant_driver_cd_live
    ON alc_api.employees (tenant_id, driver_cd)
 WHERE driver_cd IS NOT NULL AND deleted_at IS NULL;

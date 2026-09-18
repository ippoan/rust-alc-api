-- 乗務員 (employees) の二重登録を `(tenant_id, driver_cd)` で 1 行に畳む (Refs ippoan/rust-alc-api#669)
--
-- なぜ二重になったか: employees への書き込み経路が 2 つあり、キーが噛み合っていない。
--   1. theearth 乗務員マスタ同期 (crates/alc-misc/src/repo/employees.rs の upsert_by_code) は
--      **code 列**をキーに upsert する。こちらが正本で、driver_cd は入れない
--   2. dtako 運行取り込み (crates/alc-dtako/src/repo/dtako_upload.rs の upsert_driver) は
--      **driver_cd 列だけ**で既存行を探し、無ければ code を入れずに新規 INSERT する
-- 同じ乗務員CD が両方の列に同じ値で入っていても紐付かないため、取り込みのたびに
-- 「社員番号あり」「社員番号 `-`」の 2 行が管理画面に並ぶ。
--
-- この migration は既に出来てしまった重複を畳む。新規発生を止めるのは同 PR の
-- upsert_driver / get_employee_id_by_driver_cd の解決ラダー (code 優先) 側。
--
-- 方針:
--   * 突合は **同一 tenant_id 内のみ**。テナント跨ぎの突合はしない
--   * 敗者は **soft-delete (deleted_at)**。物理 DELETE はしない (この repo の削除は常に deleted_at)
--   * 既存データへの無条件 UPDATE はせず、対になる行が実在する行だけ `WHERE EXISTS` で触る
--   * 一意 index (UNIQUE (tenant_id, driver_cd)) は**張らない** — upsert_by_code に
--     「code 一致行を deleted_at = NULL で復活させる」3 本目の書き手があり、復活した行が
--     同 driver_cd の生存行と衝突すると同期バッチのトランザクションごと 500 になる。
--     復活経路を塞ぐのとセットで別 PR にする
--
-- 本番 DB に直接クエリできないので、規模は RAISE NOTICE を staging のログで見る。

DO $$
DECLARE
    backfilled_count   BIGINT;
    ops_moved          BIGINT;
    hours_moved        BIGINT;
    hours_conflicted   BIGINT;
    segments_moved     BIGINT;
    losers_deleted     BIGINT;
BEGIN
    -- 1. 正本 (code あり) に driver_cd をバックフィルする。
    --    dtako 由来の重複行 (code なし・driver_cd あり) が同じ tenant に実在するときだけ。
    UPDATE alc_api.employees e
       SET driver_cd  = e.code,
           updated_at = NOW()
     WHERE e.code IS NOT NULL
       AND e.driver_cd IS NULL
       AND e.deleted_at IS NULL
       AND EXISTS (
           SELECT 1
             FROM alc_api.employees d
            WHERE d.tenant_id = e.tenant_id
              AND d.code IS NULL
              AND d.driver_cd = e.code
              AND d.deleted_at IS NULL
       );
    GET DIAGNOSTICS backfilled_count = ROW_COUNT;

    -- 2. 勝者 / 敗者の対応表。生存行を (tenant_id, driver_cd) で grouping し、
    --    「code あり」→ 古い created_at → id の順で先頭を勝者とする。
    --    **2 行以上あるグループはすべて対象** (code 側に対が無い重複も畳む)。
    CREATE TEMP TABLE employees_dedup_map ON COMMIT DROP AS
    WITH ranked AS (
        SELECT id,
               tenant_id,
               driver_cd,
               first_value(id) OVER (
                   PARTITION BY tenant_id, driver_cd
                   ORDER BY (code IS NOT NULL) DESC, created_at, id
               ) AS winner_id,
               COUNT(*) OVER (PARTITION BY tenant_id, driver_cd) AS group_size
          FROM alc_api.employees
         WHERE deleted_at IS NULL
           AND driver_cd IS NOT NULL
    )
    SELECT tenant_id,
           driver_cd,
           id AS loser_id,
           winner_id
      FROM ranked
     WHERE group_size > 1
       AND id <> winner_id;

    -- 3. FK の付け替え (soft-delete より先)。dtako 側で driver_id を持つ 3 テーブル。

    -- 3a. dtako_operations: UNIQUE は (tenant_id, unko_no, crew_role) で driver_id を含まないため素直に付け替えられる
    UPDATE alc_api.dtako_operations o
       SET driver_id = m.winner_id
      FROM employees_dedup_map m
     WHERE o.driver_id = m.loser_id
       AND o.tenant_id = m.tenant_id;
    GET DIAGNOSTICS ops_moved = ROW_COUNT;

    -- 3b. dtako_daily_work_hours は UNIQUE (tenant_id, driver_id, work_date, start_time) を持つ
    --     (migrations/054_dtako_tables.sql:120)。勝者が同じ (work_date, start_time) の行を
    --     既に持っていると付け替えが一意制約に当たり、**migration 全体 (1 ファイル 1
    --     トランザクション) が落ちる**。衝突する行は動かさず敗者側に残す:
    --       - 敗者は soft-delete されるので、重複していた集計が帳票に二重計上されることはない
    --       - この表は取り込みのたびに (driver_id, work_date, start_time) 単位で
    --         DELETE → INSERT で作り直される (crates/alc-dtako/src/repo/dtako_upload.rs:553)
    --         ため、次回取り込みで勝者側に入り直す
    --     敗者が複数居るグループでは同じ勝者・同じ時刻へ 2 行が来ることがあるので、
    --     row_number() で 1 行だけに絞る。
    CREATE TEMP TABLE employees_dedup_hours ON COMMIT DROP AS
    SELECT h.id,
           h.tenant_id,
           m.winner_id,
           row_number() OVER (
               PARTITION BY m.winner_id, h.work_date, h.start_time ORDER BY h.id
           ) AS rn
      FROM alc_api.dtako_daily_work_hours h
      JOIN employees_dedup_map m
        ON m.loser_id = h.driver_id
       AND m.tenant_id = h.tenant_id
     WHERE NOT EXISTS (
           SELECT 1
             FROM alc_api.dtako_daily_work_hours w
            WHERE w.tenant_id = h.tenant_id
              AND w.driver_id = m.winner_id
              AND w.work_date = h.work_date
              AND w.start_time = h.start_time
       );

    UPDATE alc_api.dtako_daily_work_hours h
       SET driver_id  = d.winner_id,
           updated_at = NOW()
      FROM employees_dedup_hours d
     WHERE h.id = d.id
       AND h.tenant_id = d.tenant_id
       AND d.rn = 1;
    GET DIAGNOSTICS hours_moved = ROW_COUNT;

    SELECT COUNT(*)
      INTO hours_conflicted
      FROM alc_api.dtako_daily_work_hours h
      JOIN employees_dedup_map m
        ON m.loser_id = h.driver_id
       AND m.tenant_id = h.tenant_id;

    -- 3c. dtako_daily_work_segments は一意制約が無い (index のみ) ので素直に付け替えられる
    UPDATE alc_api.dtako_daily_work_segments s
       SET driver_id = m.winner_id
      FROM employees_dedup_map m
     WHERE s.driver_id = m.loser_id
       AND s.tenant_id = m.tenant_id;
    GET DIAGNOSTICS segments_moved = ROW_COUNT;

    -- 4. 敗者を soft-delete する (物理削除はしない)
    UPDATE alc_api.employees e
       SET deleted_at = NOW(),
           updated_at = NOW()
      FROM employees_dedup_map m
     WHERE e.id = m.loser_id
       AND e.tenant_id = m.tenant_id
       AND e.deleted_at IS NULL;
    GET DIAGNOSTICS losers_deleted = ROW_COUNT;

    -- 5. 規模をログに出す (本番 DB に直接クエリできないため staging のログで確認する)
    RAISE NOTICE 'employees dedup: driver_cd backfilled=% / losers soft-deleted=%', backfilled_count, losers_deleted;
    RAISE NOTICE 'employees dedup: driver_id moved to winner — operations=% daily_work_hours=% (left on loser by unique conflict=%) daily_work_segments=%', ops_moved, hours_moved, hours_conflicted, segments_moved;
END $$;

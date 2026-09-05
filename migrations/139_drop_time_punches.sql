-- 打刻の残置表 `time_punches` を DROP する。
-- Refs ippoan/rust-alc-api#620, #615, #616, #618, ippoan/alc-app-s3#134
--
-- 打刻の一次表は `hub_measurements` (kind='timecard') に寄せ終わっている:
--
--   * #615 — 端末打刻の ingest から `time_punches` への中継を撤去した
--     (社員解決を payload へ凍結)。以後、端末経由でこの表に行は増えない。
--   * #616 / #617 — 打刻一覧・CSV を `hub_measurements` からの導出に変えた
--     (`crates/alc-misc/src/repo/timecard.rs` の `PUNCHES_CTE`)。以後、読み手が無い。
--   * #618 — ブラウザ版 (キオスク / Android) の打刻も `hub_measurements` へ書くようにした。
--     これで最後の書き手が消えた。
--
-- 結果としてこの表は**書き手も読み手も無い**。過去の行を賃金の監査証跡として残す必要は
-- 無い (オーナー判断) ため、表ごと落とす。
--
-- **`DROP TABLE` 一発で落とす。** index (`idx_time_punches_tenant` /
-- `idx_time_punches_employee_date` / `idx_time_punches_device`)、FK
-- (`time_punches_device_id_fkey`)、RLS ポリシー (`tenant_isolation_time_punches`) は
-- 表に従属するので同時に消える。migration 036 / 041 が足した列・FK を個別に DROP するのは
-- PostgreSQL の仕様上ただの冗長なので書かない。
--
-- guard: **DROP の前に `time_punches` と `hub_measurements` (kind='timecard') の件数を
-- `RAISE NOTICE` で出し、突き合わせを deploy ログに残す。** そのうえで
-- **「最後の書き手が本番から消えたあとに増えた行」が 1 行でもあれば `RAISE EXCEPTION` で
-- 中止する。**
--
--   * 閾値を「総件数の絶対値」に置かなかった理由: migration 138 は事前の実測 (13 件) が
--     あったので上限 20 件を書けたが、この表は**事前に本番へ SQL を打たない**方針
--     (突き合わせは migration 内で行う) なので実測が無い。当てずっぽうの絶対値は
--     大きすぎれば一生発火せず、小さすぎれば正常な履歴の量で deploy を止める。
--   * 代わりに**「増えているか」を見る**。repo 内に `time_punches` への INSERT は
--     1 つも残っていない (#615 / #618) ので、期待値は厳密に 0 件であり、
--     1 件でもあれば repo の外に未知の writer がいる = まだ死んでいない表なので消さない。
--   * 基準時刻は **#618 が本番へ出たあと**に置く必要がある (merge から Release Wave の
--     flip までは旧リビジョンがまだ書けるため、#618 の merge 時刻を使うと正常な行で
--     誤発火する)。次の merge である 2ea2d720 の時刻 (2026-09-05 09:43:23+09) を採る。
--
-- **中止すると `src/bin/migrate.rs` が落ちて deploy が止まり、`_sqlx_migrations` に
-- dirty 行 (success=false) が残る。** 復旧は
-- `DELETE FROM _sqlx_migrations WHERE version = 139;` で dirty 行を消してから、
-- 書き手を潰したうえで**新しい version 番号で作り直す** (適用済みファイルは変更しない)。
--
-- **`time_punches` の RLS は `ENABLE` のみで `FORCE` ではない**
-- (`migrations/034_create_timecard.sql:30`) ため、オーナー実行のこの migration は
-- RLS をバイパスして全テナントの行を数える (件数は全テナント合計)。

DO $$
DECLARE
    punch_count      BIGINT;
    hub_count        BIGINT;
    after_cutoff     BIGINT;
BEGIN
    SELECT count(*) INTO punch_count FROM alc_api.time_punches;

    SELECT count(*) INTO hub_count
    FROM alc_api.hub_measurements
    WHERE kind = 'timecard';

    -- 最後の書き手 (#618) が本番へ出たあとに増えた行。期待値は 0。
    SELECT count(*) INTO after_cutoff
    FROM alc_api.time_punches
    WHERE created_at >= TIMESTAMPTZ '2026-09-05 09:43:23+09';

    RAISE NOTICE 'migration 139: time_punches % 行 (うち書き手撤去後に増えた行 % 件) / hub_measurements(kind=timecard) % 行',
        punch_count, after_cutoff, hub_count;

    IF after_cutoff > 0 THEN
        RAISE EXCEPTION
            'migration 139: 書き手を撤去した (#615 / #618) あとに time_punches へ % 行増えている。repo の外に writer がいる可能性があるので DROP を中止した。writer を潰してから新しい version の migration で消すこと',
            after_cutoff;
    END IF;

    DROP TABLE alc_api.time_punches;
END
$$;

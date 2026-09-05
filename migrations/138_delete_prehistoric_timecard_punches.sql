-- 1970 年に並ぶ打刻 (hub_measurements の kind='timecard') を削除する。
-- Refs ippoan/alc-app-s3#134, ippoan/alc-app-s3#144
--
-- NFC タイムカード端末が SNTP 起動 (ippoan/alc-app-s3#144) より前に送った行は、
-- `recorded_at_ms` に**起動からの稼働時間**が入っていたため 1970 起点になっている。
-- 送信時に稼働時間の差で補正する仕組み (ippoan/alc-app-s3#124) は「あとで同期したら
-- 過去ぶんを補正する」形なので、**当時 1 度も同期しなかった端末の行には永久に発火しない**。
-- 打刻一覧の絞り込みと並びは `COALESCE(recorded_at, created_at)`
-- (`crates/alc-misc/src/repo/timecard.rs` の `PUNCHES_CTE`) なので、これらは
-- 1970 年の打刻として並び続ける。賃金データなので消す (オーナー判断)。
-- 実測 2026-09-04 時点で 13 件。
--
-- 削除条件は `kind` / `recorded_at` / `created_at` の 3 つの AND。この形にした理由:
--
--   * **`device_id` / `tenant_id` をハードコードしない。** この repo は public で、
--     本番の device credential (auth-worker が発行する JWT の sub) を書くと
--     git 履歴に恒久的に残る。
--   * **`device_id` だけでは絞りにならない。** 一意制約は `UNIQUE (tenant_id, device_id, seq)`
--     (`migrations/126_create_hub_measurements.sql:29`) で device_id は全体一意ではなく、
--     `migrations/122_devices_re_pair.sql` の re-pair で同じ端末が別テナントに付く形がある。
--   * **`created_at` の上限が「測定時点で既にあった行だけ」を意味する。** これ以降に入る行には
--     構造的に当たらない (端末は #144 で SNTP 起動済みなので、以後同じ壊れ方はしない)。
--   * **`recorded_at IS NULL` は消さない。** あちらは「端末計時を送らなかった」正常な形で、
--     一覧では `created_at` (到着時刻) に倒れる。
--   * **他 kind (alcohol / temperature / blood_pressure / license) は巻き込まない。**
--     アルコールチェックは法定記録。
--
-- **`hub_measurements` の RLS は `ENABLE` のみで `FORCE` ではない**
-- (`migrations/126_create_hub_measurements.sql:34-38`) ため、オーナー実行のこの migration は
-- RLS をバイパスして全テナントの行に届く。**だから上の条件そのものが唯一の歯止め**である。
--
-- guard: DELETE と同じ 3 条件で数え、20 件を超えたら中止する (実測 13 件。桁が違うなら
-- 前提が変わっているので黙って消さない)。あわせて `created_at` の条件を外した件数を
-- NOTICE で出す — 測定時点より後にも同種の行が入っていないかを deploy ログで知るため
-- (**そちらは消さない**)。
--
-- **中止すると `src/bin/migrate.rs` が落ちて deploy が止まり、`_sqlx_migrations` に
-- dirty 行 (success=false) が残る。** 復旧は
-- `DELETE FROM _sqlx_migrations WHERE version = 138;` で dirty 行を消してから、
-- 条件を見直した migration を**新しい version 番号で作り直す** (適用済みファイルは変更しない)。

DO $$
DECLARE
    target_count    BIGINT;
    unbounded_count BIGINT;
BEGIN
    SELECT count(*) INTO target_count
    FROM alc_api.hub_measurements
    WHERE kind = 'timecard'
      AND recorded_at < TIMESTAMPTZ '2020-01-01'
      AND created_at  < TIMESTAMPTZ '2026-09-05';

    -- created_at の上限を外した件数。測定時点 (2026-09-04) より後に同種の行が
    -- 増えていないかの観測用で、削除対象ではない。
    SELECT count(*) INTO unbounded_count
    FROM alc_api.hub_measurements
    WHERE kind = 'timecard'
      AND recorded_at < TIMESTAMPTZ '2020-01-01';

    RAISE NOTICE 'migration 138: 削除対象 % 件 / created_at 上限を外した 1970 年の timecard 行 % 件',
        target_count, unbounded_count;

    IF target_count > 20 THEN
        RAISE EXCEPTION
            'migration 138: 削除対象が % 件で想定 (実測 13 件、上限 20) を超えたため中止した。前提が変わっている可能性があるので、条件を見直してから新しい migration で消すこと',
            target_count;
    END IF;

    DELETE FROM alc_api.hub_measurements
    WHERE kind = 'timecard'
      AND recorded_at < TIMESTAMPTZ '2020-01-01'
      AND created_at  < TIMESTAMPTZ '2026-09-05';
END
$$;

-- timecard_cards に「行の出所」を持たせる。Refs ippoan/rust-alc-api#644
--
-- IC カード台帳の**継続同期**の削除側 (`POST /api/timecard/cards/delete-by-card`) を
-- 作るにあたって、「どの範囲まで消してよいか」を機械が決められる根拠が要る。
-- `timecard_cards` には**この同期と無関係に alc 側で直接登録されたカードが在り得る**
-- ので、card_id だけで無条件に消すとそれを巻き込む。
--
-- **既存の `label` 列を根拠にはできない。** 穴が 3 つある:
--
--   * 自由文で誰でも書ける (`POST /timecard/cards` の body でも
--     `PUT /timecard/cards/bulk-by-code` の body でも任意の文字列が入る)
--   * 偽陰性が実装に在る — `bulk_upsert_cards_by_code` の `ON CONFLICT ... DO UPDATE` は
--     `employee_id` しか更新せず、「同じ持ち主だから何もしない」経路は 1 行も書かない。
--     つまり「同期が触った行なのに label が違う」が既に在り得る
--   * 人が UI から同じ文字列を打てば射程に入ってしまう (偽陽性)
--
-- ⇒ 別列 `source` を足し、**値はサーバ側の定数**
-- (`alc_core::repository::timecard::CARD_SOURCE_LEDGER_SYNC`) だけが入るようにする。
-- **body からは絶対に取らない。** 刻むのは一括取り込みの INSERT だけで、単発の
-- `POST /timecard/cards` は刻まない (= `NULL` のまま = 削除の射程外)。
--
-- backfill は**初回移行で使った label** (`timecard-cf-worker` の `IMPORT_LABEL`
-- = 'timecard-ic-ledger-import') の行に限る。この label は初回移行 (完了済み) が
-- 一括取り込み経由で入れたものなので、定数と同じ値を刻んでよい。
--
-- guard: 対象件数を NOTICE で出し、想定 (初回移行で入った枚数。桁は 3 桁前半)
-- の上限 200 を超えたら中止する (migration 138 と同じ形)。超える = label を人が
-- 手で書いた行が混ざっている等、前提が変わっている合図なので黙って刻まない。
--
-- **中止すると `src/bin/migrate.rs` が落ちて deploy が止まり、`_sqlx_migrations` に
-- dirty 行 (success=false) が残る。** 復旧は
-- `DELETE FROM _sqlx_migrations WHERE version = 145;` で dirty 行を消してから、
-- 条件を見直した migration を**新しい version 番号で作り直す** (適用済みファイルは変更しない)。
--
-- `source` に CHECK 制約は付けない。将来ほかの同期経路が別の定数を刻む余地を残すため
-- (削除の射程は SQL の `WHERE source = $3` 側で縛れている)。

ALTER TABLE alc_api.timecard_cards ADD COLUMN source TEXT;

DO $$
DECLARE
    target_count BIGINT;
    total_count  BIGINT;
BEGIN
    SELECT count(*) INTO target_count
    FROM alc_api.timecard_cards
    WHERE label = 'timecard-ic-ledger-import';

    SELECT count(*) INTO total_count FROM alc_api.timecard_cards;

    RAISE NOTICE 'migration 145: source を刻む対象 % 件 / timecard_cards 全体 % 件',
        target_count, total_count;

    IF target_count > 200 THEN
        RAISE EXCEPTION
            'migration 145: backfill 対象が % 件で想定 (初回移行で入った枚数、上限 200) を超えたため中止した。label は自由文なので人が手で書いた行が混ざっている可能性がある。条件を見直してから新しい migration で刻むこと',
            target_count;
    END IF;

    UPDATE alc_api.timecard_cards
    SET source = 'timecard-ic-ledger-import'
    WHERE label = 'timecard-ic-ledger-import';
END
$$;

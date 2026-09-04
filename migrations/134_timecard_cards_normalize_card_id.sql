-- timecard_cards.card_id を正規化形 (trim + 小文字 + ':' 除去) に揃える。
-- Refs ippoan/alc-app-s3#134
--
-- 同じ物理カードでも読み取り側で表記が揺れる (IDm を大文字で出す NFC タイムカード
-- 端末、小文字で出すローカル NFC ブリッジ、`AA:BB:..` と区切る実装) が、照合は
-- `WHERE tenant_id = $1 AND card_id = $2` の完全一致なので、生値のままだと同じ
-- カードが別カードとして扱われる。
--
-- **読み側 (照合) だけ正規化するのでは足りない。** `ABC` と `abc` の 2 行が同時に
-- 存在し得ると、正規化後の値がどちらにも一致して**打刻が別人に着く**。登録側
-- (alc_core::repository::timecard::normalize_card_id 経由) を揃えたうえで、
-- ここで既存行を移行し、CHECK 制約で「正規化されていない値は書けない」を固定する。
--
-- 小文字を採ったのは alc-carins の normalize_nfc_uuid (車検証 NFC タグ、
-- migration 048 の car_inspection_nfc_tags) と規約を揃えるため。

UPDATE alc_api.timecard_cards
SET card_id = lower(replace(btrim(card_id), ':', ''))
WHERE card_id <> lower(replace(btrim(card_id), ':', ''));

-- 既存の idx_timecard_cards_unique (tenant_id, card_id) が、上の UPDATE 以降は
-- そのまま「正規化後の一意性」を保証する。だから index は増やさない。
-- 同一テナントに `ABC` と `abc` が両方あった環境ではこの UPDATE が
-- idx_timecard_cards_unique の一意制約違反で落ちる (= 同じ人か別人かは機械には
-- 決められないので、黙って畳まず止める。手で片方を消してから再実行すること)。
ALTER TABLE alc_api.timecard_cards
    ADD CONSTRAINT timecard_cards_card_id_normalized
    CHECK (card_id = lower(replace(btrim(card_id), ':', '')));

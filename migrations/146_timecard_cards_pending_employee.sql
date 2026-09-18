-- 社員がまだ居ないカードも台帳に載せられるようにする。Refs ippoan/rust-alc-api#644
--
-- カード台帳の一括取り込み (`PUT /api/timecard/cards/bulk-by-code`) は社員番号
-- (`code`) をキーに社員を引くが、**社員マスタは別経路 (デジタコ relay の
-- `PUT /employees/bulk-by-code`) が別スケジュールで入れる**。新しい乗務員の
-- カードを登録すると社員がまだ居らず、取り込みは `employee_not_found` で
-- その行を落としていた。落ちた行を後から拾い直す仕組みが無いので、
-- **そのカードは永久に alc に入らない**。
--
-- ⇒ 社員が引けなかった行も受け入れる (`employee_id` を NULL 可にする) が、
-- **受け入れたら必ず後から結び付けられる**ようにする。そのために
-- `pending_employee_code` に社員番号を残す — `timecard_cards` は
-- `(tenant_id, employee_id, card_id, label, source)` しか持たず (034 / 145)、
-- `employee_id` を NULL にすると「どの社員のカードか」が行から消えるため。
-- 社員マスタ同期が同じ code の社員を入れた時点で、同じトランザクション内の
-- UPDATE (`link_pending_cards`) が結び付けて `pending_employee_code` を NULL に戻す。
--
-- **`CHECK` 制約は足さない** (ユーザー指示 2026-09-17)。
-- 「社員に結び付いていないカード」を拒否すべき場所は**カードをかざした瞬間 = 端末
-- (CoreS3)** で、そこには人が立っている。DB で弾いても呼び出し元に返るのは
-- 分かりにくい 500 だけで、その場に居る人には何も伝わらない。打刻は
-- `hub_measurements` の読み出しで社員が解決できなければ未解決のまま出るので、
-- **未結び付きカードで打刻が誰かに着くことはない** (拒否はもともとそこで起きる)。
--
-- 部分 index は「保留のカードを社員番号で引く」1 用途のためだけ。
-- 結び付いた行は `pending_employee_code` が NULL に戻るので index から落ちる。

ALTER TABLE alc_api.timecard_cards ALTER COLUMN employee_id DROP NOT NULL;
ALTER TABLE alc_api.timecard_cards ADD COLUMN pending_employee_code TEXT;

CREATE INDEX idx_timecard_cards_pending_code
    ON alc_api.timecard_cards(tenant_id, pending_employee_code)
    WHERE pending_employee_code IS NOT NULL;

-- 運行管理者による点呼 OK/NG 判定を記録する (Refs ippoan/alc-app#315)
--
-- オーナー決定:
--   1. NG でも点呼は完了扱いのまま、判定だけ記録する (status は変えない)
--   2. 判定を書けるのは運行管理者に限定する (ロール検査は API 側)
--   3. NG の理由は任意入力 (必須にしない)
--
-- 流用しないもの (調査で確認済み。crates/alc-tenko/src/tenko_sessions.rs 参照):
--   - safety_judgment: サーバが自己申告とバイタルから自動計算する値。日次健康集計
--     (repo/daily_health.rs) と webhook (tenko_webhooks.rs) が依存しており、
--     人の判断を混ぜると両方が誤発火する
--   - cancel_session: 押すと即 status='cancelled' に遷移するので決定 1 と矛盾する
--
-- manager_judgment_by は「誰が判断したか」の法定記録用。ロール検査の実測の結果
-- (Refs 親レビューの [回答])、ログイン中の tenant admin アカウント (users.role =
-- admin/viewer/payroll) は「どの運行管理者が判断したか」を表さない — alc-app の
-- 運行管理者識別は employees.role (driver/manager/admin, TEXT[]) を見る顔認証が
-- 別途担うため。よって manager_judgment_by は employees.id への FK にし、
-- 判定した運行管理者そのものを指す (API 側で role に manager/admin を含むか検証する)。
--
-- 未判定は NULL のまま (既存データの backfill はしない)。

ALTER TABLE alc_api.tenko_sessions
    ADD COLUMN IF NOT EXISTS manager_judgment TEXT
        CHECK (manager_judgment IN ('ok', 'ng')),
    ADD COLUMN IF NOT EXISTS manager_judgment_reason TEXT,
    ADD COLUMN IF NOT EXISTS manager_judgment_by UUID REFERENCES alc_api.employees(id);

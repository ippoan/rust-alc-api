-- 運行 (dtako_operations) を上げ直す・手動で消すときの「前の値」を残す変更記録
-- (Refs ohishi-exp/nuxt-dtako-admin#1133 訴訟用の準備ページ)
--
-- 同じ運行を上げ直すと dtako_operations の行は DELETE → INSERT されるため、
-- 前の値がどこにも残らない。消す直前に旧行を読み、新行と比べて違うときだけ
-- ここへ 1 行残す (reason = 'reupload')。管理画面からの手動削除
-- (DELETE /operations/{unko_no}) も行が消えるので、after = NULL で残す
-- (reason = 'manual_delete')。記録はこの migration を適用した日から。
--
-- before / after の JSON キー:
--   driver_cd / departure_at / return_at            … dtako_operations (+ employees) の値
--   drive_minutes / cargo_minutes / break_minutes / rest_minutes
--                                                   … KUDGIVT の区間時間をイベントCD の
--                                                     既定分類で合計したもの (上げ直しのみ。
--                                                     before は split 済みの R2 旧 KUDGIVT
--                                                     から出すので、取れなければキーごと無い)
--
-- 追記専用の記録なので UPDATE / DELETE の権限は渡さない。
CREATE TABLE alc_api.dtako_operation_changes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    unko_no TEXT NOT NULL,
    crew_role INTEGER NOT NULL,
    driver_cd TEXT,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    upload_id UUID,
    reason TEXT NOT NULL CHECK (reason IN ('reupload', 'manual_delete')),
    before JSONB,
    after JSONB
);

CREATE INDEX idx_dtako_operation_changes_driver
    ON alc_api.dtako_operation_changes(tenant_id, driver_cd, recorded_at);

ALTER TABLE alc_api.dtako_operation_changes ENABLE ROW LEVEL SECURITY;
CREATE POLICY dtako_operation_changes_tenant ON alc_api.dtako_operation_changes
    USING (tenant_id = current_setting('app.current_tenant_id')::UUID);

-- GRANT ON ALL TABLES は既存表のスナップショットなので新表に効かない
GRANT SELECT, INSERT ON alc_api.dtako_operation_changes TO alc_api_app;

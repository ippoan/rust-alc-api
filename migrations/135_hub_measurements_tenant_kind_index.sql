-- 打刻履歴を hub_measurements から導出する (Refs ippoan/alc-app-s3#134) ための index。
--
-- 既存の hub_measurements_tenant_device は (tenant_id, device_id, created_at DESC) で
-- **先頭の次が device_id** なので、「テナント内の kind='timecard' を新しい順に」には
-- 効かない。ingest テーブルは伸び続けるため、index 無しだとテナント内全走査になる。
--
-- `GET /api/hub/measurements?kind=` の絞り込みにも同じ index が効くので、
-- partial (WHERE kind IN (...)) にはしていない。列は 3 つとも小さい。
--
-- CONCURRENTLY にしていないのは migration がトランザクション内で走るため
-- (CREATE INDEX CONCURRENTLY はトランザクション内で実行できない)。
CREATE INDEX hub_measurements_tenant_kind
    ON alc_api.hub_measurements(tenant_id, kind, created_at DESC);

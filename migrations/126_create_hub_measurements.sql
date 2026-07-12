-- CoreS3 (alc-app-s3) ハブ測定データの ingest テーブル。
-- cf-alc-recorder (Cloudflare Worker) が WebSocket で受けた測定を
-- auth-worker /alc-internal-proxy 経由 POST /api/hub/measurements で永続化する。
--
-- 経路: CoreS3 →(WSS + device JWT)→ cf-alc-recorder →(service binding)→
--       auth-worker →(OIDC + X-Internal-Shared-Secret + X-Tenant-ID)→ 本 API
--
-- Refs ippoan/rust-alc-api#564 / ippoan/alc-app#106 / ippoan/auth-worker#363

CREATE TABLE alc_api.hub_measurements (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL REFERENCES alc_api.tenants(id),
    -- auth-worker device credential の device_id (JWT sub)。cf-alc-recorder が
    -- introspect 済み claims から注入する (ペイロード値は信用しない)。
    device_id   TEXT NOT NULL,
    -- temperature / blood_pressure / alcohol (端末パース済み) / fc1200_raw (hex fallback)。
    -- 将来 kind が増える (timecard イベント等の構想あり) ため DB CHECK は張らず、
    -- allowlist はアプリ側 (alc-devices hub_measurements::HUB_MEASUREMENT_KINDS) で検証する。
    kind        TEXT NOT NULL,
    -- ble-medical-gateway 互換 JSON をそのまま格納。
    payload     JSONB NOT NULL,
    -- device 内シーケンス (再送冪等性のキー)。
    seq         BIGINT NOT NULL,
    -- 端末計時 (CoreS3 の recorded_at_ms 由来)。時計未同期端末では NULL。
    recorded_at TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- 再送重複の排除 (ON CONFLICT DO NOTHING)。
    UNIQUE (tenant_id, device_id, seq)
);

CREATE INDEX hub_measurements_tenant_device
    ON alc_api.hub_measurements(tenant_id, device_id, created_at DESC);

ALTER TABLE alc_api.hub_measurements ENABLE ROW LEVEL SECURITY;

-- tenant スコープ。INSERT/UPDATE/DELETE は WITH CHECK で明示的に同じ tenant 内のみ
-- (dtako_tickets と同形、splinter の RLS 推奨に従う)。
CREATE POLICY hub_measurements_tenant ON alc_api.hub_measurements
    USING (tenant_id = current_setting('app.current_tenant_id')::UUID)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id')::UUID);

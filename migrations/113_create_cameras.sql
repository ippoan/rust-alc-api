-- 監視カメラ死活管理ドメイン (Refs #345)。
-- 事業所に設置された Tapo 等の ONVIF カメラの死活監視。alc-app (拠点タブレット) が
-- ONVIF GetSystemDateAndTime を周期実行し、結果を camera_health_logs に記録する。
-- 連続失敗を alc-camera-api が集約して alc-trouble に障害を自動起票する。
--
-- 設計判断 (issue の「未検討事項」への回答):
-- * ONVIF 認証情報 (user/pass) は保持しない。GetSystemDateAndTime のみなら認証不要。
--   将来 PTZ/stream を扱う際に Secret Manager 連携で別途設計する。
-- * 障害自動起票の冪等性は cameras.active_down_ticket_id で担保 (down 中の重複起票防止)。
--   復旧 (alive) 時はリンクをクリアするのみで ticket は自動クローズしない (手動クローズ)。
-- * office は既存の alc_api.trouble_offices を参照する (専用 offices テーブルは作らない)。

-- カメラマスタ
CREATE TABLE alc_api.cameras (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    office_id UUID REFERENCES alc_api.trouble_offices(id),
    name TEXT NOT NULL DEFAULT '',
    -- LAN 内アドレス。検証は軽いが sqlx の ipnetwork feature を workspace 全体に
    -- 波及させないため INET ではなく TEXT で保持する。
    ip TEXT NOT NULL DEFAULT '',
    onvif_port INTEGER NOT NULL DEFAULT 2020,
    model TEXT NOT NULL DEFAULT '',
    active BOOLEAN NOT NULL DEFAULT TRUE,
    -- down 検知で自動起票した未解決 ticket。down 中の重複起票防止 + 復旧でクリア。
    active_down_ticket_id UUID REFERENCES alc_api.trouble_tickets(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_cameras_tenant ON alc_api.cameras(tenant_id);
CREATE INDEX idx_cameras_tenant_active ON alc_api.cameras(tenant_id, active) WHERE active = TRUE;

ALTER TABLE alc_api.cameras ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON alc_api.cameras
    FOR ALL USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);
GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.cameras TO alc_api_app;

-- ヘルスチェックログ (件数が伸びるので checked_at で retention 削除する想定)
CREATE TABLE alc_api.camera_health_logs (
    id BIGSERIAL PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    camera_id UUID NOT NULL REFERENCES alc_api.cameras(id) ON DELETE CASCADE,
    alive BOOLEAN NOT NULL,
    latency_ms INTEGER,
    error TEXT,
    checked_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    source_device_id TEXT
);

-- 「直近 N 件」を引く + retention 削除に効くインデックス
CREATE INDEX idx_camera_health_logs_camera_checked
    ON alc_api.camera_health_logs(tenant_id, camera_id, checked_at DESC);
CREATE INDEX idx_camera_health_logs_checked ON alc_api.camera_health_logs(checked_at);

ALTER TABLE alc_api.camera_health_logs ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON alc_api.camera_health_logs
    FOR ALL USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);
GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.camera_health_logs TO alc_api_app;
GRANT USAGE, SELECT ON SEQUENCE alc_api.camera_health_logs_id_seq TO alc_api_app;

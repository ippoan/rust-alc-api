-- 056: スクレイプ履歴テーブル (daiun-salary から移行)
CREATE TABLE IF NOT EXISTS alc_api.dtako_scrape_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES alc_api.tenants(id),
    target_date DATE NOT NULL,
    comp_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'running',
    message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_dtako_scrape_history_tenant_date
    ON alc_api.dtako_scrape_history(tenant_id, created_at DESC);
ALTER TABLE alc_api.dtako_scrape_history ENABLE ROW LEVEL SECURITY;
CREATE POLICY dtako_scrape_history_tenant ON alc_api.dtako_scrape_history
    USING (tenant_id = current_setting('app.current_tenant_id')::UUID);

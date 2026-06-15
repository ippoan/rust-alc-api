-- dtako (デジタコ) SD カードエラー等の通知メール起票テーブル。
-- ippoan/email-receiver Worker から POST /api/dtako/tickets で起票され、
-- dtako-scraper の F-VOS3020 設定 ZIP DL 結果を PATCH で反映し、
-- 現場作業者が QR スキャンで close する pipeline 用。
--
-- Refs ippoan/email-receiver#1 / ippoan/rust-alc-api#414

CREATE TABLE alc_api.dtako_tickets (
    id                       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id                UUID NOT NULL REFERENCES alc_api.tenants(id),
    source                   TEXT NOT NULL DEFAULT 'email'
                              CHECK (source IN ('email', 'manual')),
    source_email_subject     TEXT,
    source_email_from        TEXT,
    source_email_message_id  TEXT,
    source_email_received_at TIMESTAMPTZ NOT NULL,
    vehicle_name             TEXT NOT NULL,
    vehicle_code             TEXT,
    error_kind               TEXT NOT NULL,
    status                   TEXT NOT NULL DEFAULT 'open'
                              CHECK (status IN ('open', 'scraping', 'scraped', 'closed')),
    comp_id                  TEXT,
    unko_no                  TEXT,
    operation_started_at     TIMESTAMPTZ,
    operation_ended_at       TIMESTAMPTZ,
    scraped_payload          JSONB,
    settings_zip_r2_key      TEXT,
    close_token              TEXT NOT NULL UNIQUE,
    closed_at                TIMESTAMPTZ,
    closed_by                TEXT,
    raw_email_text           TEXT,
    created_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at               TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX dtako_tickets_tenant_status
    ON alc_api.dtako_tickets(tenant_id, status);
CREATE INDEX dtako_tickets_close_token
    ON alc_api.dtako_tickets(close_token);
CREATE INDEX dtako_tickets_received_at
    ON alc_api.dtako_tickets(tenant_id, source_email_received_at DESC);

ALTER TABLE alc_api.dtako_tickets ENABLE ROW LEVEL SECURITY;

-- tenant スコープ。INSERT/UPDATE/DELETE は WITH CHECK で明示的に同じ tenant 内のみ
-- (splinter の RLS 推奨に従う)。
CREATE POLICY dtako_tickets_tenant ON alc_api.dtako_tickets
    USING (tenant_id = current_setting('app.current_tenant_id')::UUID)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id')::UUID);

-- close 経路は close_token (= 推測不可な URL-safe 32 byte hex) で
-- SECURITY DEFINER 関数経由にする。
-- tenant_id 解決は token から逆引きで行うため、関数内では RLS を bypass する。
CREATE OR REPLACE FUNCTION alc_api.close_dtako_ticket_by_token(
    p_token TEXT,
    p_closed_by TEXT
) RETURNS UUID
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = alc_api
AS $$
DECLARE
    v_id UUID;
BEGIN
    UPDATE alc_api.dtako_tickets
       SET status     = 'closed',
           closed_at  = now(),
           closed_by  = p_closed_by,
           updated_at = now()
     WHERE close_token = p_token
       AND status     <> 'closed'
    RETURNING id INTO v_id;
    RETURN v_id;
END;
$$;

-- 関数の execute 権限は alc_api_app (= ランタイムロール) に明示付与。
GRANT EXECUTE ON FUNCTION alc_api.close_dtako_ticket_by_token(TEXT, TEXT)
    TO alc_api_app;

-- 通常テーブルの GRANT (alc_api_app は NOBYPASSRLS、RLS で tenant 分離される)。
GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.dtako_tickets TO alc_api_app;

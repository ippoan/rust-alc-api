-- theearth の DVR (ドライブレコーダー) 動画通知の受け皿。
-- dtako-scraper-relay の cron が theearth から一覧を取り、
-- POST /api/dvr/notifications で行を起票 → POST /api/dvr/files/{id} で
-- .vdf の実体を R2 (dtako バケット) へ保存する。
--
-- 経路: theearth →(cron scrape)→ dtako-scraper-relay
--       →(service binding)→ auth-worker
--       →(X-Internal-Shared-Secret + X-Tenant-ID)→ 本 API →(R2)
--
-- Refs ohishi-exp/nuxt-dtako-admin#1094
--
-- 重複判定は自然キー UNIQUE (tenant_id, serial_no, file_name)。
-- 動画 URL (source_url) は relay 側が 2 段のリクエストから組み立てる派生値で、
-- theearth の形式が変わると全件が新規扱いになるため、**キーにしない**
-- (列として保持するだけ)。
--
-- vehicle_cd / vehicle_name / driver_name / event_type は表示用の付随情報で、
-- theearth の一覧に欠けることがあり得る。欠損を 400 で弾く価値が無いので
-- NULL 許容にし、アプリ側も Option で受ける。
CREATE TABLE alc_api.dvr_notifications (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    UUID NOT NULL REFERENCES alc_api.tenants(id),
    -- DVR 機器のシリアル。theearth 由来 = untrusted なので、長さと文字種は
    -- アプリ側 (alc-dtako dvr_notifications::valid_key_component) で検証する。
    serial_no    TEXT NOT NULL,
    -- .vdf のファイル名。R2 key の末尾に入るため同上の検証を通す。
    file_name    TEXT NOT NULL,
    vehicle_cd   TEXT,
    vehicle_name TEXT,
    driver_name  TEXT,
    event_type   TEXT,
    -- theearth が示す録画日時。パースできない / 欠ける場合は NULL。
    dvr_datetime TIMESTAMPTZ,
    -- relay が組み立てた動画 URL。取得元の記録用で、重複判定には使わない。
    source_url   TEXT,
    -- pending … 行はあるが実体未保存 / stored … R2 保存済 / failed … 恒久失敗
    file_status  TEXT NOT NULL DEFAULT 'pending'
                  CHECK (file_status IN ('pending', 'stored', 'failed')),
    -- 実体保存の試行回数。ingest 応答が「まだ再送する価値がある pending」を
    -- 選ぶ閾値に使う (アプリ側の MAX_FILE_ATTEMPTS)。
    attempts     INTEGER NOT NULL DEFAULT 0,
    r2_key       TEXT,
    size_bytes   BIGINT,
    last_error   TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- 再取り込みの冪等キー (ON CONFLICT DO NOTHING)。
    UNIQUE (tenant_id, serial_no, file_name)
);

-- file_status の partial index は張らない。実測 15 件/日 (8 ヶ月) 規模で、
-- かつ「pending を全件走査する」問い合わせがコード側に無い
-- (ingest は自然キーの UNIQUE index で 1 行ずつ引き、file 保存は id + tenant_id
-- で引く)。索引を足しても使う経路が無く、書き込みコストだけが増える。

ALTER TABLE alc_api.dvr_notifications ENABLE ROW LEVEL SECURITY;

-- tenant スコープ。INSERT/UPDATE/DELETE は WITH CHECK で明示的に同じ tenant 内のみ
-- (dtako_tickets / hub_measurements と同形、splinter の RLS 推奨に従う)。
CREATE POLICY dvr_notifications_tenant ON alc_api.dvr_notifications
    USING (tenant_id = current_setting('app.current_tenant_id')::UUID)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id')::UUID);

-- 通常テーブルの GRANT (alc_api_app は NOBYPASSRLS、RLS で tenant 分離される)。
-- `GRANT ON ALL TABLES` は既存表のスナップショットで新表には効かないため、
-- 表を足した migration が自分で GRANT を書く (migration 115 / 125 と同形)。
GRANT SELECT, INSERT, UPDATE, DELETE ON alc_api.dvr_notifications TO alc_api_app;

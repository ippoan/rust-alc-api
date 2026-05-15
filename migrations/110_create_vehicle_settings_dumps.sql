-- vehicle_settings_dumps: nuxt-dtako-admin が R2 に保存した車輛設定 dump の
-- メタデータを追跡するテーブル。実体 (json + cfg) は R2 上にあり、
-- 本テーブルは 「どの車輛にいつ dump が上がったか」 を高速に取り出す
-- インデックスとして使う (フロントが R2 list せず 1 query で未確認車輛や
-- 履歴を取れるようにするため)。
--
-- 関連:
--   - ohishi-exp/nuxt-dtako-admin#38 / #39 / #40 / #41
--   - ippoan/rust-alc-api#347

CREATE TABLE IF NOT EXISTS alc_api.vehicle_settings_dumps (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID NOT NULL,
    -- dtako_vehicles.vehicle_cd と同じキー。FK は張らない (車輛マスタ未登録
    -- の車輛も dump だけはアップロードされうるため、緊さずにしておく)
    vehicle_cd      VARCHAR(32) NOT NULL,
    -- NET780 dump dir 名 例: "20260514_093253-0-0-4437"
    dump_dir        VARCHAR(64) NOT NULL,
    machine_id      VARCHAR(32),
    firm_main_app   VARCHAR(32),
    -- R2 上の key (フロントが GET /api/vehicle-settings/object?key=... で使う)
    r2_json_key     TEXT NOT NULL,
    r2_cfg_key      TEXT NOT NULL,
    uploaded_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- アップロードしたユーザ (alc_users.id)。現状フロントが送らないので nullable。
    uploaded_by     UUID,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- 同じ dump_dir を同じ vehicle に二重 INSERT しないための一意制約。
    -- フロントが同じ zip を 2 回投げたときは ON CONFLICT で処理する。
    UNIQUE (tenant_id, vehicle_cd, dump_dir)
);

-- 履歴クエリ (車輛別、新しい順ソート)
CREATE INDEX IF NOT EXISTS idx_vsd_tenant_vehicle_uploaded
    ON alc_api.vehicle_settings_dumps (tenant_id, vehicle_cd, uploaded_at DESC);

-- 集計クエリ (テナント全体、新しい順)
CREATE INDEX IF NOT EXISTS idx_vsd_tenant_uploaded
    ON alc_api.vehicle_settings_dumps (tenant_id, uploaded_at DESC);

COMMENT ON TABLE alc_api.vehicle_settings_dumps IS
  '車輛設定 (NET780 *.cfg) の R2 dump メタデータ。実体は R2 、本表は index 用';

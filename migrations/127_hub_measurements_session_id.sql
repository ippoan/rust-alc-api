-- hub_measurements に点呼セッション識別子を足す (Refs ippoan/alc-app-s3#112)。
--
-- migration 126 の時点では 1 行 1 測定しか持たず、「どの行が同じ点呼のものか」を
-- 判別する手段が経路のどこにも無かった (CoreS3 の UplinkRecord にも
-- cf-alc-recorder にも識別子が乗っていない)。タブレット経由の通常フローなら
-- 点呼の単位は tenko_sessions 側にあるが、LAN/PoE 構成でタブレットを外すと
-- 点呼の単位を持つ主体がいなくなるため、端末 (CoreS3) が発番する。
--
-- 値は端末が「1 回の点呼 (UI の Measuring → Idle)」ごとに 1 つ発番する文字列で、
-- **端末内でのみ一意** (実装はセッション開始時の seq)。グローバルな一意性は
-- (tenant_id, device_id, session_id) の組で担保する。
--
-- NULL 許容にする理由 (後方互換):
--   - 既存行 (この migration より前に入った測定) には値が無い
--   - 旧ファームは送ってこない
--   - 点呼外の単発計測 (待機画面で BLE 機器から届いたもの) は意図的に NULL
-- よって「NULL = セッション不明」であり、欠損ではない。

ALTER TABLE alc_api.hub_measurements ADD COLUMN session_id TEXT;

-- セッション単位の絞り込み用。NULL 行 (点呼外・旧データ) を引く用途は無いので
-- partial index にして、伸び続ける ingest テーブルの index を小さく保つ。
-- created_at DESC を末尾に付けるのは一覧の並びが常にこの順のため
-- (hub_measurements_tenant_device と同じ方針)。
CREATE INDEX hub_measurements_tenant_session
    ON alc_api.hub_measurements(tenant_id, device_id, session_id, created_at DESC)
    WHERE session_id IS NOT NULL;

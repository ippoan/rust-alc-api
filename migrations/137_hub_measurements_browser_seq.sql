-- ブラウザ版 (キオスク / Android) の打刻を hub_measurements へ入れるための seq 採番。
-- Refs ippoan/alc-app-s3#134
--
-- 打刻の一次表は hub_measurements で、`UNIQUE (tenant_id, device_id, seq)` が
-- 再送の冪等キー。**端末は NVS の連番を持つが、ブラウザは持たない** ので
-- サーバが採番する。
--
-- `MAX(seq) + 1` にしないのは、同時打刻で競合して unique 違反のリトライループが
-- 要るため。sequence なら 1 回の nextval で衝突しない。一意なのは
-- (tenant_id, device_id, seq) の組なので、グローバルな 1 本で足りる
-- (値が飛んでも疎でも構わない — 読み出しは seq を順序に使わない)。
--
-- **冪等性はこの経路には無い** (端末と違い「同じ seq の再送」が無いため)。
-- ブラウザの二度押しは 2 行になる。従来の time_punches への INSERT も同じだったので
-- 退行ではないが、冪等が要るなら呼び出し側で連打を止めること。
CREATE SEQUENCE alc_api.hub_measurements_browser_seq;

-- **GRANT は必須。** alc_api_app は所有者ではないので、これが無いと nextval が
-- permission denied になる。**pg のテストは所有者で繋ぐため通ってしまい、
-- 本番だけ 500 になる**壊れ方をする (migration 132 の GRANT と同じ理由)。
GRANT USAGE, SELECT ON SEQUENCE alc_api.hub_measurements_browser_seq TO alc_api_app;

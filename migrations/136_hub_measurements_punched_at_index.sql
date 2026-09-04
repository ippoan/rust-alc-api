-- 打刻一覧を hub_measurements から導出する (Refs ippoan/alc-app-s3#134) ための index。
--
-- 一覧は「打刻時刻」= COALESCE(recorded_at, created_at) で絞り、その順に並べる。
--
-- **なぜ created_at で絞らないのか**: 端末は回線断のあいだ打刻を NVS のキューに
-- 溜め、復帰後にまとめて送る。created_at は「サーバに届いた時刻」なので、朝の
-- 打刻が夕方の行として並んでしまう。ユーザーが見たいのは「いつタップしたか」で、
-- それは recorded_at (端末計時) 側。**時計未同期の端末では recorded_at が NULL に
-- なりうる**ので COALESCE で created_at に倒す (migration 126 の列コメント参照)。
--
-- したがって式インデックスが要る。135 の (tenant_id, kind, created_at DESC) は
-- GET /api/hub/measurements の絞り込み用で、こちらの並び順には効かない。
CREATE INDEX hub_measurements_tenant_kind_punched_at
    ON alc_api.hub_measurements(tenant_id, kind, (COALESCE(recorded_at, created_at)) DESC);

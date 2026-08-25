-- internal 経路 (auth-worker → require_internal_jwt) からの LINE WORKS 送信で、
-- channel 行 id だけを手がかりに tenant ごと解決するための SECURITY DEFINER lookup。
--
-- 背景: 無人 worker (dtako-scraper-relay の netprint cron) が予約番号を LINE WORKS
-- のトークルームへ通知したい。既存の tenant 経路 `POST /notify/lineworks/channels/
-- {id}/test-send` は `require_tenant_header` 配下で、auth-worker の
-- alc-internal-proxy は tenant 経路の forward を禁じている (shared secret だけで
-- X-Tenant-ID を詐称でき #434 の再現になるため)。よって internal 経路に送信 API を
-- 足すが、internal 経路は X-Tenant-ID を honor しないので tenant は DB 側で解決する。
--
-- lineworks_channels は FORCE ROW LEVEL SECURITY (migration 102) なので、実行ロール
-- alc_api_app が tenant GUC 無しで素引きしても 0 行になる。trouble_schedules の
-- get_trouble_schedule / notify_deliveries の lookup_delivery_for_view と同じく、
-- SECURITY DEFINER 関数で RLS をバイパスして 1 行返し、呼び出し側は返った
-- tenant_id を明示して以降の tenant スコープ取得 (bot_configs) を行う。
--
-- active = FALSE (Bot が退出済み) の行も返す。ここで 404 にすると「id が無い」と
-- 「Bot が居ない」を呼び出し側が区別できなくなるため、送信は上流 LINE WORKS API
-- に到達させて 502 として loud に失敗させる。
--
-- splinter 対策として SET search_path = alc_api を付与 (既存の SECURITY DEFINER
-- 関数群と同形)。
CREATE OR REPLACE FUNCTION alc_api.lookup_lineworks_channel_for_send(p_id UUID)
RETURNS SETOF alc_api.lineworks_channels
LANGUAGE sql SECURITY DEFINER SET search_path = alc_api AS $$
  SELECT * FROM alc_api.lineworks_channels WHERE id = p_id LIMIT 1;
$$;

GRANT EXECUTE ON FUNCTION alc_api.lookup_lineworks_channel_for_send(UUID) TO alc_api_app;

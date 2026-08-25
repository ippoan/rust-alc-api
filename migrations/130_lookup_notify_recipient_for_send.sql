-- internal 経路 (auth-worker → require_internal_jwt) からの LINE WORKS 送信で、
-- notify_recipients の行 id だけを手がかりに tenant ごと解決するための
-- SECURITY DEFINER lookup。migration 129 (lookup_lineworks_channel_for_send) の
-- recipient 版。
--
-- 背景: 無人 worker (dtako-scraper-relay の netprint cron) が日報の予約番号を
-- LINE WORKS へ通知する口として #596 で `POST /api/internal/lineworks/send` を
-- 足したが、実運用ではトークルーム (lineworks_channels) が 1 件も登録されて
-- おらず (Bot が招待されていない)、宛先は notify_recipients の個人だった。
-- そこで同じエンドポイントの body を `recipient_id` でも受けられるように広げる
-- (新パスを足すと auth-worker の alc-internal-proxy allowlist にも PR が要る)。
--
-- internal 経路は X-Tenant-ID を honor しない (shared secret だけで tenant を
-- 詐称でき #434 の再現になる) ため、tenant は DB 側で行から解決する。実行ロール
-- alc_api_app は notify_recipients の非所有者なので tenant GUC 無しの素引きは
-- RLS (migration 069) で 0 行になる。所有者として動く SECURITY DEFINER 関数で
-- 1 行だけ返し、呼び出し側は返った tenant_id を明示して以降の tenant スコープ
-- 取得 (bot_configs) を行う — find_recipient_by_line_user_id (081/119) /
-- lookup_lineworks_channel_for_send (129) と同じ作法。
--
-- enabled = FALSE の行も返す。ここで 0 行にすると「id が無い」と「宛先が無効化
-- されている」をハンドラが区別できず、どちらも 404 に潰れてしまう。無効な宛先へ
-- 送らない判断はハンドラ側 (400 recipient_disabled) が行う。
--
-- splinter 対策として SET search_path = alc_api を付与 (既存の SECURITY DEFINER
-- 関数群と同形)。
CREATE OR REPLACE FUNCTION alc_api.lookup_notify_recipient_for_send(p_id UUID)
RETURNS SETOF alc_api.notify_recipients
LANGUAGE sql SECURITY DEFINER SET search_path = alc_api AS $$
  SELECT * FROM alc_api.notify_recipients WHERE id = p_id LIMIT 1;
$$;

GRANT EXECUTE ON FUNCTION alc_api.lookup_notify_recipient_for_send(UUID) TO alc_api_app;

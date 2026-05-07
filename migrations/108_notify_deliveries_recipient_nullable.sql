-- notify_deliveries.recipient_id を NULL 許容に変更。
--
-- 動機: PDF redact テスト用エンドポイント (POST /api/notify/test/redact-pdf) で
-- 「受信者を指定せずに公開閲覧 URL だけ生成する」ユースケースを許可するため。
-- viewer.rs の lookup_delivery_for_view (migration 107) は recipient_id を参照しないので
-- RLS ポリシーや既存配信ロジックへの副作用なし。
--
-- 既存の通常配信パスでは recipient_id を必ず埋める運用は維持。

ALTER TABLE alc_api.notify_deliveries
    ALTER COLUMN recipient_id DROP NOT NULL;

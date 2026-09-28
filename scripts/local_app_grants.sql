-- 本番 (Supabase) の alc_api_app の GRANT をテスト DB で再現する (Refs #685)。
--
-- 本番の初期の表の GRANT は migration の外 (Supabase 上で手動) で付いたので、
-- migration だけではテスト DB の alc_api_app の権限が本番とずれる。
-- tests/common/mod.rs の migrate_and_grant が migration の直後に流す。
--
-- 規則:
--   * 本番の一覧をそのまま写す。GRANT ... ON ALL TABLES で一律に付けない
--     (本番で権限を絞っている表まで広げてしまうため)
--   * 新しい表の GRANT はここではなく、その表を作る migration に書く
--     (本番へ届くのは migration だけ。付け忘れは tests/app_role_grants_test.rs が落とす)
--   * 冪等 (何度流しても同じ結果)
--
-- TODO(#685): 暫定。本番で確認済みの「alc_api の全表に SELECT」だけを、migration が
-- GRANT を書いていない表に付けている。本番の表・sequence・function の一覧で置き換える。

GRANT SELECT ON alc_api._sqlx_migrations TO alc_api_app;
GRANT SELECT ON alc_api.carrying_item_vehicle_conditions TO alc_api_app;
GRANT SELECT ON alc_api.carrying_items TO alc_api_app;
GRANT SELECT ON alc_api.communication_items TO alc_api_app;
GRANT SELECT ON alc_api.device_registration_requests TO alc_api_app;
GRANT SELECT ON alc_api.devices TO alc_api_app;
GRANT SELECT ON alc_api.dtako_daily_work_hours TO alc_api_app;
GRANT SELECT ON alc_api.dtako_daily_work_segments TO alc_api_app;
GRANT SELECT ON alc_api.dtako_event_classifications TO alc_api_app;
GRANT SELECT ON alc_api.dtako_offices TO alc_api_app;
GRANT SELECT ON alc_api.dtako_operations TO alc_api_app;
GRANT SELECT ON alc_api.dtako_scrape_history TO alc_api_app;
GRANT SELECT ON alc_api.dtako_upload_history TO alc_api_app;
GRANT SELECT ON alc_api.dtako_vehicles TO alc_api_app;
GRANT SELECT ON alc_api.employees TO alc_api_app;
GRANT SELECT ON alc_api.guidance_record_attachments TO alc_api_app;
GRANT SELECT ON alc_api.guidance_records TO alc_api_app;
GRANT SELECT ON alc_api.hub_measurements TO alc_api_app;
GRANT SELECT ON alc_api.measurements TO alc_api_app;
GRANT SELECT ON alc_api.tenant_allowed_emails TO alc_api_app;
GRANT SELECT ON alc_api.tenants TO alc_api_app;
GRANT SELECT ON alc_api.tenko_carrying_item_checks TO alc_api_app;
GRANT SELECT ON alc_api.tenko_records TO alc_api_app;
GRANT SELECT ON alc_api.tenko_schedules TO alc_api_app;
GRANT SELECT ON alc_api.tenko_sessions TO alc_api_app;
GRANT SELECT ON alc_api.timecard_cards TO alc_api_app;
GRANT SELECT ON alc_api.users TO alc_api_app;
GRANT SELECT ON alc_api.vehicle_settings_dumps TO alc_api_app;
GRANT SELECT ON alc_api.webhook_configs TO alc_api_app;
GRANT SELECT ON alc_api.webhook_deliveries TO alc_api_app;

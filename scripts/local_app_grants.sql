-- 本番 (Supabase) の alc_api_app の GRANT をテスト DB で再現する (Refs #685)。
--
-- 本番の実態をそのまま写したもの (2026-09-28 に SQL Editor で取得。表 85 / 関数 20 / sequence 12)。
-- migration の GRANT より広い (全表 ALL 相当、dtako_operation_changes・_sqlx_migrations も含む)。
-- 本番側を絞るかは別 issue で扱う — ここは「本番に揃える」ことだけが目的。
--
-- 本番の初期の表の GRANT は migration の外 (Supabase 上で手動) で付いたので、migration だけでは
-- テスト DB の alc_api_app の権限が本番とずれる。tests/common/mod.rs の migrate_and_grant が
-- migration の直後に流す。
--
-- 規則:
--   * 本番の一覧をそのまま写す。GRANT ... ON ALL TABLES で一律に付けない
--   * 新しい表の GRANT はここではなく、その表を作る migration に書く
--     (本番へ届くのは migration だけ。付け忘れは tests/app_role_grants_test.rs が落とす)
--   * 冪等 (何度流しても同じ結果)。本番にしか無い対象 (ローカルに無い表・関数・sequence) は
--     存在確認して飛ばし、NOTICE に名前を出す
--   * 関数は型だけで引く (GRANT は引数名を見ない。init_local_db.sql の resolve_sso_config は
--     引数名が本番と違う)

DO $grants$
DECLARE
    tbl_privs CONSTANT text := 'DELETE, INSERT, REFERENCES, SELECT, TRIGGER, TRUNCATE, UPDATE';
    seq_privs CONSTANT text := 'SELECT, UPDATE, USAGE';
    tbl text;
    fn text;
    seq text;
    skipped text[] := '{}';
BEGIN
    FOREACH tbl IN ARRAY ARRAY[
        '_sqlx_migrations',
        'access_requests',
        'api_tokens',
        'bot_configs',
        'camera_health_logs',
        'cameras',
        'car_inspection',
        'car_inspection_deregistration',
        'car_inspection_deregistration_files',
        'car_inspection_files',
        'car_inspection_files_a',
        'car_inspection_files_b',
        'car_inspection_nfc_tags',
        'carrying_item_vehicle_conditions',
        'carrying_items',
        'communication_items',
        'device_registration_requests',
        'devices',
        'dtako_daily_work_hours',
        'dtako_daily_work_segments',
        'dtako_event_classifications',
        'dtako_offices',
        'dtako_operation_changes',
        'dtako_operations',
        'dtako_scrape_history',
        'dtako_tickets',
        'dtako_upload_history',
        'dtako_vehicles',
        'dtakologs',
        'dvr_notifications',
        'employee_health_baselines',
        'employees',
        'equipment_failures',
        'file_access_logs',
        'files',
        'files_append',
        'guidance_record_attachments',
        'guidance_records',
        'hub_measurements',
        'item_files',
        'items',
        'lineworks_channels',
        'maintenance_categories',
        'maintenance_files',
        'maintenance_records',
        'maintenance_vehicles',
        'measurements',
        'notify_deliveries',
        'notify_documents',
        'notify_groups',
        'notify_line_configs',
        'notify_recipient_groups',
        'notify_recipients',
        'pending_car_inspection_pdfs',
        'sso_provider_configs',
        'tenant_allowed_emails',
        'tenants',
        'tenko_call_drivers',
        'tenko_call_logs',
        'tenko_call_numbers',
        'tenko_carrying_item_checks',
        'tenko_records',
        'tenko_schedules',
        'tenko_sessions',
        'timecard_cards',
        'trouble_categories',
        'trouble_custom_field_defs',
        'trouble_field_layouts',
        'trouble_files',
        'trouble_notification_prefs',
        'trouble_offices',
        'trouble_progress_statuses',
        'trouble_schedules',
        'trouble_status_history',
        'trouble_task_statuses',
        'trouble_task_types',
        'trouble_tasks',
        'trouble_tickets',
        'trouble_workflow_states',
        'trouble_workflow_transitions',
        'users',
        'vehicle_settings_dumps',
        'vein_templates',
        'webhook_configs',
        'webhook_deliveries'
    ] LOOP
        IF to_regclass('alc_api.' || tbl) IS NULL THEN
            skipped := skipped || ('table ' || tbl);
        ELSE
            EXECUTE format('GRANT %s ON alc_api.%I TO alc_api_app', tbl_privs, tbl);
        END IF;
    END LOOP;

    FOREACH fn IN ARRAY ARRAY[
        'archive_delete_dtako_date(text, text)',
        'archive_fetch_dtako_rows_json(text, text, bigint, bigint)',
        'archive_list_dtako_dates()',
        'archive_list_old_dtako_dates(text)',
        'archive_upsert_dtako_batch(jsonb)',
        'close_dtako_ticket_by_token(text, text)',
        'find_recipient_by_line_user_id(text)',
        'find_user_by_line_user_id(text)',
        'get_device_re_pair_state(uuid)',
        'get_device_settings_by_id(uuid)',
        'get_trouble_schedule(uuid)',
        'list_enabled_line_configs()',
        'lookup_bot_config_for_webhook(text)',
        'lookup_delivery_for_view(uuid)',
        'lookup_device_tenant(uuid)',
        'lookup_lineworks_channel_for_send(uuid)',
        'lookup_notify_recipient_for_send(uuid)',
        'mark_delivery_read(uuid)',
        'resolve_sso_config(text, text)',
        'verify_device_token(uuid, uuid)'
    ] LOOP
        IF to_regprocedure('alc_api.' || fn) IS NULL THEN
            skipped := skipped || ('function ' || fn);
        ELSE
            EXECUTE format('GRANT EXECUTE ON FUNCTION alc_api.%s TO alc_api_app', fn);
        END IF;
    END LOOP;

    FOREACH seq IN ARRAY ARRAY[
        'camera_health_logs_id_seq',
        'car_inspection_deregistration_files_id_seq',
        'car_inspection_deregistration_id_seq',
        'car_inspection_id_seq',
        'car_inspection_nfc_tags_id_seq',
        'file_access_logs_id_seq',
        'hub_measurements_browser_seq',
        'pending_car_inspection_pdfs_id_seq',
        'tenko_call_drivers_id_seq',
        'tenko_call_logs_id_seq',
        'tenko_call_numbers_id_seq',
        'trouble_tickets_ticket_no_seq'
    ] LOOP
        IF to_regclass('alc_api.' || seq) IS NULL THEN
            skipped := skipped || ('sequence ' || seq);
        ELSE
            EXECUTE format('GRANT %s ON SEQUENCE alc_api.%I TO alc_api_app', seq_privs, seq);
        END IF;
    END LOOP;

    RAISE NOTICE 'local_app_grants: skipped % (%)', coalesce(array_length(skipped, 1), 0), array_to_string(skipped, ', ');
END
$grants$;

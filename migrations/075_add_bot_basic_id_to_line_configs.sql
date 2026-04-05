-- notify_line_configs に bot_basic_id カラム追加
ALTER TABLE alc_api.notify_line_configs
    ADD COLUMN bot_basic_id TEXT;
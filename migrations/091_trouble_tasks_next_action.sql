ALTER TABLE alc_api.trouble_tasks
    ADD COLUMN next_action TEXT NOT NULL DEFAULT '',
    ADD COLUMN next_action_by UUID,
    ADD COLUMN next_action_due TIMESTAMPTZ;

ALTER TABLE alc_api.trouble_schedules
    ADD COLUMN task_id UUID REFERENCES alc_api.trouble_tasks(id) ON DELETE CASCADE;

CREATE INDEX idx_trouble_schedules_task ON alc_api.trouble_schedules(task_id);

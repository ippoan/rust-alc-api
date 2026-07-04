-- 状況管理 (trouble_tasks) の印刷時、ユーザーが任意の行の直前に手動で改ページを
-- 指定できるようにする。チケットのタスクデータとして保存するため、どの端末/
-- ブラウザから印刷しても同じページ割りになる。

ALTER TABLE alc_api.trouble_tasks
    ADD COLUMN print_page_break_before BOOLEAN NOT NULL DEFAULT false;

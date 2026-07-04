-- トラブルチケットに「相手方車両」「賞罰委員会」の自由記述フィールドを追加。
ALTER TABLE alc_api.trouble_tickets
    ADD COLUMN counterparty_vehicle TEXT NOT NULL DEFAULT '';
ALTER TABLE alc_api.trouble_tickets
    ADD COLUMN disciplinary_committee TEXT NOT NULL DEFAULT '';

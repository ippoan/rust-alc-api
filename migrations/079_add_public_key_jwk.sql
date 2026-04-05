-- 公開鍵 JWK を保存して設定画面で再表示可能にする
ALTER TABLE alc_api.notify_line_configs
    ADD COLUMN IF NOT EXISTS public_key_jwk TEXT;

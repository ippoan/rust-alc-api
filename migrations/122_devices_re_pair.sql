-- kiosk 端末 re-pair (再認証)。Refs #495、設計 SoT: docs/plan-device-repair.md
--
-- 管理者が時限 window を開け (re_pair_authorized_until)、端末がその window 内で
-- device credential を再取得する (POST /api/devices/re-pair)。値そのものは
-- auth-worker から端末に直行させ、rust 側は判定に必要な状態のみ保持する。

ALTER TABLE alc_api.devices
    ADD COLUMN re_pair_authorized_until TIMESTAMPTZ,
    ADD COLUMN last_re_pair_at TIMESTAMPTZ,
    ADD COLUMN re_pair_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN hardware_id TEXT;

-- 端末向け re-pair 判定用の状態取得 (認証不要エンドポイントから device_id のみで
-- 参照する)。063/114 の get_device_settings_by_id と同じ理由でテーブル直 SELECT
-- は許可しない (device_select_by_id USING(true) は 063 で撤去済み) ため
-- SECURITY DEFINER 関数を追加する。
CREATE FUNCTION alc_api.get_device_re_pair_state(p_device_id UUID)
RETURNS TABLE(
    tenant_id UUID,
    status TEXT,
    re_pair_authorized_until TIMESTAMPTZ,
    last_re_pair_at TIMESTAMPTZ,
    hardware_id TEXT,
    settings_token UUID
)
LANGUAGE sql SECURITY DEFINER SET search_path = alc_api
AS $$
    SELECT tenant_id, status, re_pair_authorized_until, last_re_pair_at,
           hardware_id, settings_token
    FROM alc_api.devices WHERE id = p_device_id;
$$;
GRANT EXECUTE ON FUNCTION alc_api.get_device_re_pair_state(UUID) TO alc_api_app;

-- Archive operations: SECURITY DEFINER functions + RLS policies
-- alc_api_app (NOBYPASSRLS) がアーカイブ処理でテナント横断 SELECT/DELETE できるようにする

-- RLS policies: archive_mode が true の場合のみ全テナントアクセス許可
CREATE POLICY archive_select ON alc_api.dtakologs
  FOR SELECT USING (current_setting('alc_api.archive_mode', true) = 'true');

CREATE POLICY archive_delete ON alc_api.dtakologs
  FOR DELETE USING (current_setting('alc_api.archive_mode', true) = 'true');

CREATE POLICY archive_insert ON alc_api.dtakologs
  FOR INSERT WITH CHECK (current_setting('alc_api.archive_mode', true) = 'true');

CREATE POLICY archive_update ON alc_api.dtakologs
  FOR UPDATE USING (current_setting('alc_api.archive_mode', true) = 'true');

-- 1. list_dtako_dates
CREATE OR REPLACE FUNCTION alc_api.archive_list_dtako_dates()
RETURNS TABLE(tenant_id TEXT, date_str TEXT, row_count BIGINT)
LANGUAGE plpgsql SECURITY DEFINER SET search_path = alc_api AS $$
BEGIN
  PERFORM set_config('alc_api.archive_mode', 'true', true);
  RETURN QUERY
    SELECT d.tenant_id::TEXT, d.data_date_time::DATE::TEXT, COUNT(*)
    FROM alc_api.dtakologs d
    GROUP BY d.tenant_id, d.data_date_time::DATE
    ORDER BY d.data_date_time::DATE;
END;
$$;

-- 2. fetch_dtako_rows_json
CREATE OR REPLACE FUNCTION alc_api.archive_fetch_dtako_rows_json(
  p_tenant_id TEXT, p_date TEXT, p_limit BIGINT, p_offset BIGINT
)
RETURNS TABLE(row_json JSONB)
LANGUAGE plpgsql SECURITY DEFINER SET search_path = alc_api AS $$
BEGIN
  PERFORM set_config('alc_api.archive_mode', 'true', true);
  RETURN QUERY
    SELECT row_to_json(d)::JSONB
    FROM alc_api.dtakologs d
    WHERE d.tenant_id = p_tenant_id::UUID AND d.data_date_time::DATE = p_date::DATE
    ORDER BY d.data_date_time
    LIMIT p_limit OFFSET p_offset;
END;
$$;

-- 3. list_old_dtako_dates
CREATE OR REPLACE FUNCTION alc_api.archive_list_old_dtako_dates(p_cutoff TEXT)
RETURNS TABLE(tenant_id TEXT, date_str TEXT, row_count BIGINT)
LANGUAGE plpgsql SECURITY DEFINER SET search_path = alc_api AS $$
BEGIN
  PERFORM set_config('alc_api.archive_mode', 'true', true);
  RETURN QUERY
    SELECT d.tenant_id::TEXT, d.data_date_time::DATE::TEXT, COUNT(*)
    FROM alc_api.dtakologs d
    WHERE d.data_date_time::DATE < p_cutoff::DATE
    GROUP BY d.tenant_id, d.data_date_time::DATE
    ORDER BY d.data_date_time::DATE;
END;
$$;

-- 4. delete_dtako_date
CREATE OR REPLACE FUNCTION alc_api.archive_delete_dtako_date(p_tenant_id TEXT, p_date TEXT)
RETURNS BIGINT
LANGUAGE plpgsql SECURITY DEFINER SET search_path = alc_api AS $$
DECLARE
  affected BIGINT;
BEGIN
  PERFORM set_config('alc_api.archive_mode', 'true', true);
  DELETE FROM alc_api.dtakologs
  WHERE tenant_id = p_tenant_id::UUID AND data_date_time::DATE = p_date::DATE;
  GET DIAGNOSTICS affected = ROW_COUNT;
  RETURN affected;
END;
$$;

-- 5. upsert_dtako_batch
CREATE OR REPLACE FUNCTION alc_api.archive_upsert_dtako_batch(p_rows JSONB)
RETURNS VOID
LANGUAGE plpgsql SECURITY DEFINER SET search_path = alc_api AS $$
BEGIN
  PERFORM set_config('alc_api.archive_mode', 'true', true);
  INSERT INTO alc_api.dtakologs (
    tenant_id, data_date_time, vehicle_cd,
    type, all_state_font_color_index, all_state_ryout_color,
    branch_cd, branch_name, current_work_cd, data_filter_type,
    disp_flag, driver_cd, gps_direction, gps_enable,
    gps_latitude, gps_longitude, gps_satellite_num,
    operation_state, recive_event_type, recive_packet_type,
    recive_work_cd, revo, setting_temp, setting_temp1,
    setting_temp3, setting_temp4, speed, sub_driver_cd,
    temp_state, vehicle_name,
    address_disp_c, address_disp_p, all_state, all_state_ex,
    all_state_font_color, comu_date_time, current_work_name,
    driver_name, event_val, gps_lati_and_long, odometer,
    recive_type_color_name, recive_type_name,
    start_work_date_time, state, state1, state2, state3,
    state_flag, temp1, temp2, temp3, temp4,
    vehicle_icon_color, vehicle_icon_label_for_datetime,
    vehicle_icon_label_for_driver, vehicle_icon_label_for_vehicle
  )
  SELECT
    (j->>'tenant_id')::UUID, j->>'data_date_time', (j->>'vehicle_cd')::INTEGER,
    COALESCE(j->>'type', ''),
    COALESCE((j->>'all_state_font_color_index')::INTEGER, 0),
    COALESCE(j->>'all_state_ryout_color', 'Transparent'),
    COALESCE((j->>'branch_cd')::INTEGER, 0), COALESCE(j->>'branch_name', ''),
    COALESCE((j->>'current_work_cd')::INTEGER, 0),
    COALESCE((j->>'data_filter_type')::INTEGER, 0),
    COALESCE((j->>'disp_flag')::INTEGER, 0), COALESCE((j->>'driver_cd')::INTEGER, 0),
    COALESCE((j->>'gps_direction')::DOUBLE PRECISION, 0),
    COALESCE((j->>'gps_enable')::INTEGER, 0),
    COALESCE((j->>'gps_latitude')::DOUBLE PRECISION, 0),
    COALESCE((j->>'gps_longitude')::DOUBLE PRECISION, 0),
    COALESCE((j->>'gps_satellite_num')::INTEGER, 0),
    COALESCE((j->>'operation_state')::INTEGER, 0),
    COALESCE((j->>'recive_event_type')::INTEGER, 0),
    COALESCE((j->>'recive_packet_type')::INTEGER, 0),
    COALESCE((j->>'recive_work_cd')::INTEGER, 0),
    COALESCE((j->>'revo')::INTEGER, 0),
    COALESCE(j->>'setting_temp', ''), COALESCE(j->>'setting_temp1', ''),
    COALESCE(j->>'setting_temp3', ''), COALESCE(j->>'setting_temp4', ''),
    COALESCE((j->>'speed')::REAL, 0), COALESCE((j->>'sub_driver_cd')::INTEGER, 0),
    COALESCE((j->>'temp_state')::INTEGER, 0), COALESCE(j->>'vehicle_name', ''),
    j->>'address_disp_c', j->>'address_disp_p', j->>'all_state', j->>'all_state_ex',
    j->>'all_state_font_color', j->>'comu_date_time', j->>'current_work_name',
    j->>'driver_name', j->>'event_val', j->>'gps_lati_and_long', j->>'odometer',
    j->>'recive_type_color_name', j->>'recive_type_name',
    j->>'start_work_date_time', j->>'state', j->>'state1',
    j->>'state2', j->>'state3', j->>'state_flag',
    j->>'temp1', j->>'temp2', j->>'temp3', j->>'temp4',
    j->>'vehicle_icon_color', j->>'vehicle_icon_label_for_datetime',
    j->>'vehicle_icon_label_for_driver', j->>'vehicle_icon_label_for_vehicle'
  FROM jsonb_array_elements(p_rows) AS j
  ON CONFLICT (tenant_id, data_date_time, vehicle_cd) DO UPDATE SET
    type = EXCLUDED.type, speed = EXCLUDED.speed,
    gps_latitude = EXCLUDED.gps_latitude, gps_longitude = EXCLUDED.gps_longitude,
    gps_direction = EXCLUDED.gps_direction;
END;
$$;

-- GRANT EXECUTE to alc_api_app
GRANT EXECUTE ON FUNCTION alc_api.archive_list_dtako_dates() TO alc_api_app;
GRANT EXECUTE ON FUNCTION alc_api.archive_fetch_dtako_rows_json(TEXT, TEXT, BIGINT, BIGINT) TO alc_api_app;
GRANT EXECUTE ON FUNCTION alc_api.archive_list_old_dtako_dates(TEXT) TO alc_api_app;
GRANT EXECUTE ON FUNCTION alc_api.archive_delete_dtako_date(TEXT, TEXT) TO alc_api_app;
GRANT EXECUTE ON FUNCTION alc_api.archive_upsert_dtako_batch(JSONB) TO alc_api_app;

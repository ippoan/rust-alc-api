use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use super::TenantConn;

pub use alc_core::repository::devices::*;

// ============================================================
// Pg Implementation
// ============================================================

pub struct PgDeviceRepository {
    pool: PgPool,
}

impl PgDeviceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DeviceRepository for PgDeviceRepository {
    // --- Public ---

    async fn code_exists(&self, code: &str) -> Result<bool, sqlx::Error> {
        let row = sqlx::query_as::<_, (bool,)>(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM device_registration_requests
                WHERE registration_code = $1 AND status = 'pending'
                  AND (expires_at IS NULL OR expires_at > NOW())
            )
            "#,
        )
        .bind(code)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    async fn create_registration_request(
        &self,
        code: &str,
        device_name: &str,
    ) -> Result<CreateRegistrationResult, sqlx::Error> {
        let row = sqlx::query_as::<_, (String, String)>(
            r#"
            INSERT INTO device_registration_requests
                (registration_code, flow_type, device_name, status, expires_at)
            VALUES ($1, 'qr_temp', $2, 'pending', NOW() + INTERVAL '10 minutes')
            RETURNING registration_code, expires_at::text
            "#,
        )
        .bind(code)
        .bind(device_name)
        .fetch_one(&self.pool)
        .await?;
        Ok(CreateRegistrationResult {
            registration_code: row.0,
            expires_at: row.1,
        })
    }

    async fn get_registration_status(
        &self,
        code: &str,
    ) -> Result<Option<RegistrationStatusRow>, sqlx::Error> {
        let row = sqlx::query_as::<
            _,
            (
                String,
                Option<Uuid>,
                Option<Uuid>,
                Option<String>,
                Option<String>,
            ),
        >(
            r#"
            SELECT status, device_id, tenant_id, expires_at::text,
                   NULLIF(device_name, '') AS device_name
            FROM device_registration_requests
            WHERE registration_code = $1
            "#,
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| RegistrationStatusRow {
            status: r.0,
            device_id: r.1,
            tenant_id: r.2,
            expires_at: r.3,
            device_name: r.4,
        }))
    }

    async fn is_expired(&self, expires_at: &str) -> Result<bool, sqlx::Error> {
        let row = sqlx::query_as::<_, (bool,)>("SELECT $1::timestamptz < NOW()")
            .bind(expires_at)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0)
    }

    async fn find_claim_request(&self, code: &str) -> Result<Option<ClaimLookupRow>, sqlx::Error> {
        let row = sqlx::query_as::<_, (Uuid, String, Option<Uuid>, String, Option<String>, Option<String>, bool, bool)>(
            r#"
            SELECT id, flow_type, tenant_id, status, expires_at::text, device_name, is_device_owner, is_dev_device
            FROM device_registration_requests
            WHERE registration_code = $1
            "#,
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| ClaimLookupRow {
            id: r.0,
            flow_type: r.1,
            tenant_id: r.2,
            status: r.3,
            expires_at: r.4,
            device_name: r.5,
            is_device_owner: r.6,
            is_dev_device: r.7,
        }))
    }

    async fn claim_url_flow(
        &self,
        tenant_id: Uuid,
        device_name: &str,
        phone_number: Option<&str>,
        is_device_owner: bool,
        is_dev_device: bool,
        req_id: Uuid,
    ) -> Result<Uuid, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        crate::db::tenant::set_current_tenant(&mut tx, &tenant_id.to_string()).await?;

        let device_id = sqlx::query_as::<_, (Uuid,)>(
            r#"
            INSERT INTO devices (tenant_id, device_name, device_type, phone_number, status, approved_at, is_device_owner, is_dev_device)
            VALUES ($1, $2, 'android', $3, 'active', NOW(), $4, $5)
            RETURNING id
            "#,
        )
        .bind(tenant_id)
        .bind(device_name)
        .bind(phone_number)
        .bind(is_device_owner)
        .bind(is_dev_device)
        .fetch_one(&mut *tx)
        .await?
        .0;

        sqlx::query(
            r#"
            UPDATE device_registration_requests
            SET status = 'approved', device_id = $1, phone_number = $2, device_name = $3
            WHERE id = $4
            "#,
        )
        .bind(device_id)
        .bind(phone_number)
        .bind(device_name)
        .bind(req_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(device_id)
    }

    async fn claim_update_permanent_qr(
        &self,
        req_id: Uuid,
        phone_number: Option<&str>,
        device_name: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE device_registration_requests
            SET phone_number = $1, device_name = $2
            WHERE id = $3
            "#,
        )
        .bind(phone_number)
        .bind(device_name)
        .bind(req_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_device_settings(
        &self,
        device_id: Uuid,
    ) -> Result<Option<DeviceSettingsRow>, sqlx::Error> {
        sqlx::query_as::<_, DeviceSettingsRow>(
            "SELECT * FROM alc_api.get_device_settings_by_id($1)",
        )
        .bind(device_id)
        .fetch_optional(&self.pool)
        .await
    }

    async fn lookup_device_tenant(&self, device_id: Uuid) -> Result<Option<Uuid>, sqlx::Error> {
        let row = sqlx::query_as::<_, (Option<Uuid>,)>("SELECT alc_api.lookup_device_tenant($1)")
            .bind(device_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.and_then(|r| r.0))
    }

    async fn update_fcm_token(
        &self,
        device_id: Uuid,
        tenant_id: Uuid,
        fcm_token: &str,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query("UPDATE devices SET fcm_token = $1, updated_at = NOW() WHERE id = $2")
            .bind(fcm_token)
            .bind(device_id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(())
    }

    async fn update_last_login(
        &self,
        device_id: Uuid,
        tenant_id: Uuid,
        employee_id: Uuid,
        employee_name: &str,
        employee_role: &[String],
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query(
            "UPDATE devices SET last_login_employee_id = $1, last_login_employee_name = $2, last_login_employee_role = $3, updated_at = NOW() WHERE id = $4",
        )
        .bind(employee_id)
        .bind(employee_name)
        .bind(employee_role)
        .bind(device_id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }

    async fn list_fcm_devices(&self) -> Result<Vec<FcmDeviceRow>, sqlx::Error> {
        sqlx::query_as::<_, FcmDeviceRow>(
            "SELECT id, fcm_token, call_enabled, call_schedule FROM devices WHERE fcm_token IS NOT NULL AND status = 'active'",
        )
        .fetch_all(&self.pool)
        .await
    }

    async fn get_device_tenant_active(
        &self,
        device_id: Uuid,
    ) -> Result<Option<DeviceTenantRow>, sqlx::Error> {
        sqlx::query_as::<_, DeviceTenantRow>(
            "SELECT tenant_id FROM alc_api.devices WHERE id = $1 AND status = 'active'",
        )
        .bind(device_id)
        .fetch_optional(&self.pool)
        .await
    }

    async fn list_tenant_fcm_tokens_except(
        &self,
        tenant_id: Uuid,
        exclude_device_id: Uuid,
    ) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String,)>(
            "SELECT fcm_token FROM alc_api.devices WHERE tenant_id = $1 AND id != $2 AND status = 'active' AND fcm_token IS NOT NULL",
        )
        .bind(tenant_id)
        .bind(exclude_device_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    async fn list_all_callable_devices(&self) -> Result<Vec<FcmTestDeviceRow>, sqlx::Error> {
        sqlx::query_as::<_, FcmTestDeviceRow>(
            "SELECT id, device_name, fcm_token FROM alc_api.devices WHERE status = 'active' AND fcm_token IS NOT NULL AND call_enabled = true",
        )
        .fetch_all(&self.pool)
        .await
    }

    async fn update_watchdog_state(
        &self,
        device_id: Uuid,
        tenant_id: Uuid,
        running: bool,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query("UPDATE devices SET watchdog_running = $1, updated_at = NOW() WHERE id = $2")
            .bind(running)
            .bind(device_id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(())
    }

    async fn report_version(
        &self,
        device_id: Uuid,
        tenant_id: Uuid,
        version_code: i32,
        version_name: &str,
        is_device_owner: bool,
        is_dev_device: bool,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query(
            r#"UPDATE devices
               SET app_version_code = $1, app_version_name = $2,
                   is_device_owner = $3, is_dev_device = $4,
                   app_version_reported_at = NOW(), updated_at = NOW()
               WHERE id = $5"#,
        )
        .bind(version_code)
        .bind(version_name)
        .bind(is_device_owner)
        .bind(is_dev_device)
        .bind(device_id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }

    async fn list_dev_device_tenant_ids(&self) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT tenant_id::text FROM alc_api.devices WHERE status = 'active' AND is_dev_device = true AND fcm_token IS NOT NULL",
        )
        .fetch_all(&self.pool)
        .await
    }

    // --- Tenant-scoped ---

    async fn list_devices(&self, tenant_id: Uuid) -> Result<Vec<DeviceRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, DeviceRow>(
            r#"
            SELECT id, tenant_id, device_name, device_type, phone_number, user_id, status,
                   approved_by, approved_at::text, last_seen_at::text,
                   call_enabled, call_schedule, fcm_token,
                   last_login_employee_id, last_login_employee_name, last_login_employee_role,
                   app_version_code, app_version_name, is_device_owner, is_dev_device,
                   always_on, watchdog_running, created_at::text, updated_at::text
            FROM devices
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn list_pending(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<RegistrationRequestRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, RegistrationRequestRow>(
            r#"
            SELECT id, registration_code, flow_type, tenant_id, phone_number, device_name,
                   status, device_id, expires_at::text, is_device_owner, is_dev_device, created_at::text
            FROM device_registration_requests
            WHERE status = 'pending'
              AND (tenant_id = $1 OR tenant_id IS NULL)
              AND (expires_at IS NULL OR expires_at > NOW())
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn create_url_token(
        &self,
        tenant_id: Uuid,
        code: &str,
        device_name: &str,
        is_device_owner: bool,
        is_dev_device: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO device_registration_requests
                (registration_code, flow_type, tenant_id, device_name, status, expires_at, is_device_owner, is_dev_device)
            VALUES ($1, 'url', $2, $3, 'pending', NOW() + INTERVAL '24 hours', $4, $5)
            "#,
        )
        .bind(code)
        .bind(tenant_id)
        .bind(device_name)
        .bind(is_device_owner)
        .bind(is_dev_device)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn create_device_owner_token(
        &self,
        tenant_id: Uuid,
        code: &str,
        device_name: &str,
        is_dev_device: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO device_registration_requests
                (registration_code, flow_type, tenant_id, device_name, status, is_device_owner, is_dev_device)
            VALUES ($1, 'device_owner', $2, $3, 'pending', true, $4)
            "#,
        )
        .bind(code)
        .bind(tenant_id)
        .bind(device_name)
        .bind(is_dev_device)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn create_permanent_qr(
        &self,
        tenant_id: Uuid,
        code: &str,
        device_name: &str,
        is_device_owner: bool,
        is_dev_device: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO device_registration_requests
                (registration_code, flow_type, tenant_id, device_name, status, is_device_owner, is_dev_device)
            VALUES ($1, 'qr_permanent', $2, $3, 'pending', $4, $5)
            "#,
        )
        .bind(code)
        .bind(tenant_id)
        .bind(device_name)
        .bind(is_device_owner)
        .bind(is_dev_device)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_approve_request(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<ApproveLookupRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let row = sqlx::query_as::<
            _,
            (
                Uuid,
                String,
                Option<String>,
                Option<String>,
                String,
                bool,
                bool,
            ),
        >(
            r#"
            SELECT id, flow_type, phone_number, device_name, status, is_device_owner, is_dev_device
            FROM device_registration_requests
            WHERE id = $1 AND status = 'pending'
            "#,
        )
        .bind(id)
        .fetch_optional(&mut *tc.conn)
        .await?;
        Ok(row.map(|r| ApproveLookupRow {
            id: r.0,
            flow_type: r.1,
            phone_number: r.2,
            device_name: r.3,
            status: r.4,
            is_device_owner: r.5,
            is_dev_device: r.6,
        }))
    }

    async fn approve_device(
        &self,
        tenant_id: Uuid,
        req_id: Uuid,
        device_name: &str,
        device_type: &str,
        phone_number: Option<&str>,
        approved_by: Option<Uuid>,
        is_device_owner: bool,
        is_dev_device: bool,
    ) -> Result<Uuid, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        crate::db::tenant::set_current_tenant(&mut tx, &tenant_id.to_string()).await?;

        let device_id = sqlx::query_as::<_, (Uuid,)>(
            r#"
            INSERT INTO devices (tenant_id, device_name, device_type, phone_number, status, approved_by, approved_at, is_device_owner, is_dev_device)
            VALUES ($1, $2, $3, $4, 'active', $5, NOW(), $6, $7)
            RETURNING id
            "#,
        )
        .bind(tenant_id)
        .bind(device_name)
        .bind(device_type)
        .bind(phone_number)
        .bind(approved_by)
        .bind(is_device_owner)
        .bind(is_dev_device)
        .fetch_one(&mut *tx)
        .await?
        .0;

        sqlx::query(
            r#"
            UPDATE device_registration_requests
            SET status = 'approved', device_id = $1, tenant_id = COALESCE(tenant_id, $2)
            WHERE id = $3
            "#,
        )
        .bind(device_id)
        .bind(tenant_id)
        .bind(req_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(device_id)
    }

    async fn find_approve_by_code_request(
        &self,
        tenant_id: Uuid,
        code: &str,
    ) -> Result<Option<ApproveLookupRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let row = sqlx::query_as::<
            _,
            (
                Uuid,
                String,
                Option<String>,
                Option<String>,
                String,
                bool,
                bool,
            ),
        >(
            r#"
            SELECT id, flow_type, phone_number, device_name, status, is_device_owner, is_dev_device
            FROM device_registration_requests
            WHERE registration_code = $1 AND status = 'pending'
              AND (expires_at IS NULL OR expires_at > NOW())
            "#,
        )
        .bind(code)
        .fetch_optional(&mut *tc.conn)
        .await?;
        Ok(row.map(|r| ApproveLookupRow {
            id: r.0,
            flow_type: r.1,
            phone_number: r.2,
            device_name: r.3,
            status: r.4,
            is_device_owner: r.5,
            is_dev_device: r.6,
        }))
    }

    async fn approve_by_code(
        &self,
        tenant_id: Uuid,
        req_id: Uuid,
        device_name: &str,
        device_type: &str,
        phone_number: Option<&str>,
        approved_by: Option<Uuid>,
        is_device_owner: bool,
        is_dev_device: bool,
    ) -> Result<Uuid, sqlx::Error> {
        // Same logic as approve_device — create device + update request in tx
        let mut tx = self.pool.begin().await?;
        crate::db::tenant::set_current_tenant(&mut tx, &tenant_id.to_string()).await?;

        let device_id = sqlx::query_as::<_, (Uuid,)>(
            r#"
            INSERT INTO devices (tenant_id, device_name, device_type, phone_number, status, approved_by, approved_at, is_device_owner, is_dev_device)
            VALUES ($1, $2, $3, $4, 'active', $5, NOW(), $6, $7)
            RETURNING id
            "#,
        )
        .bind(tenant_id)
        .bind(device_name)
        .bind(device_type)
        .bind(phone_number)
        .bind(approved_by)
        .bind(is_device_owner)
        .bind(is_dev_device)
        .fetch_one(&mut *tx)
        .await?
        .0;

        sqlx::query(
            r#"
            UPDATE device_registration_requests
            SET status = 'approved', device_id = $1, tenant_id = COALESCE(tenant_id, $2)
            WHERE id = $3
            "#,
        )
        .bind(device_id)
        .bind(tenant_id)
        .bind(req_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(device_id)
    }

    async fn reject_device(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query(
            r#"
            UPDATE device_registration_requests
            SET status = 'rejected'
            WHERE id = $1 AND status = 'pending'
            "#,
        )
        .bind(id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn disable_device(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query(
            "UPDATE devices SET status = 'disabled', updated_at = NOW() WHERE id = $1 AND status = 'active'",
        )
        .bind(id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn enable_device(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query(
            "UPDATE devices SET status = 'active', updated_at = NOW() WHERE id = $1 AND status = 'disabled'",
        )
        .bind(id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn delete_device(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query("DELETE FROM devices WHERE id = $1")
            .bind(id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn update_call_settings(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        call_enabled: bool,
        call_schedule: Option<&serde_json::Value>,
        always_on: Option<bool>,
    ) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let result = sqlx::query(
            "UPDATE devices SET call_enabled = $1, call_schedule = $2, always_on = COALESCE($3, always_on), updated_at = NOW() WHERE id = $4",
        )
        .bind(call_enabled)
        .bind(call_schedule)
        .bind(always_on)
        .bind(id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn get_fcm_token_bypass_rls(
        &self,
        device_id: Uuid,
    ) -> Result<Option<Option<String>>, sqlx::Error> {
        let row = sqlx::query_as::<_, (Option<String>,)>(
            "SELECT fcm_token FROM alc_api.devices WHERE id = $1",
        )
        .bind(device_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.0))
    }

    async fn get_device_fcm_token(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Option<String>>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let tid = tenant_id.to_string();
        let row = sqlx::query_as::<_, (Option<String>,)>(
            "SELECT fcm_token FROM devices WHERE id = $1 AND tenant_id = $2::uuid",
        )
        .bind(id)
        .bind(&tid)
        .fetch_optional(&mut *tc.conn)
        .await?;
        Ok(row.map(|r| r.0))
    }

    async fn list_tenant_fcm_devices(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<FcmTestDeviceRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let tid = tenant_id.to_string();
        sqlx::query_as::<_, FcmTestDeviceRow>(
            "SELECT id, device_name, fcm_token FROM devices WHERE tenant_id = $1::uuid AND status = 'active' AND fcm_token IS NOT NULL",
        )
        .bind(&tid)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn list_ota_devices(
        &self,
        tenant_id: Uuid,
        dev_only: bool,
    ) -> Result<Vec<OtaDeviceRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let tid = tenant_id.to_string();
        let query = if dev_only {
            r#"SELECT id, device_name, fcm_token, app_version_code
               FROM devices
               WHERE tenant_id = $1::uuid AND status = 'active'
                 AND fcm_token IS NOT NULL AND is_dev_device = true"#
        } else {
            r#"SELECT id, device_name, fcm_token, app_version_code
               FROM devices
               WHERE tenant_id = $1::uuid AND status = 'active'
                 AND fcm_token IS NOT NULL"#
        };
        sqlx::query_as::<_, OtaDeviceRow>(query)
            .bind(&tid)
            .fetch_all(&mut *tc.conn)
            .await
    }
}

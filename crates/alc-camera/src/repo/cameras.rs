//! `CamerasRepository` の PostgreSQL 実装 (RLS テナント分離)。

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::repository::cameras::*;
use alc_core::tenant::TenantConn;

pub struct PgCamerasRepository {
    pool: PgPool,
}

impl PgCamerasRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CamerasRepository for PgCamerasRepository {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<Camera>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Camera>("SELECT * FROM cameras ORDER BY name, created_at")
            .fetch_all(&mut *tc.conn)
            .await
    }

    async fn get(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Camera>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Camera>("SELECT * FROM cameras WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tc.conn)
            .await
    }

    async fn create(&self, tenant_id: Uuid, input: &CreateCamera) -> Result<Camera, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Camera>(
            r#"
            INSERT INTO cameras (tenant_id, office_id, name, ip, onvif_port, model)
            VALUES ($1, $2, $3, $4, COALESCE($5, 2020), $6)
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(input.office_id)
        .bind(&input.name)
        .bind(&input.ip)
        .bind(input.onvif_port)
        .bind(&input.model)
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        input: &UpdateCamera,
    ) -> Result<Option<Camera>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        // COALESCE で「指定された項目だけ更新」する (NULL 引数は既存値を維持)。
        sqlx::query_as::<_, Camera>(
            r#"
            UPDATE cameras SET
                office_id = COALESCE($2, office_id),
                name = COALESCE($3, name),
                ip = COALESCE($4, ip),
                onvif_port = COALESCE($5, onvif_port),
                model = COALESCE($6, model),
                active = COALESCE($7, active),
                updated_at = now()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(id)
        .bind(input.office_id)
        .bind(input.name.as_ref())
        .bind(input.ip.as_ref())
        .bind(input.onvif_port)
        .bind(input.model.as_ref())
        .bind(input.active)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn delete(&self, tenant_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        let res = sqlx::query("DELETE FROM cameras WHERE id = $1")
            .bind(id)
            .execute(&mut *tc.conn)
            .await?;
        Ok(res.rows_affected() > 0)
    }

    async fn insert_health_log(
        &self,
        tenant_id: Uuid,
        camera_id: Uuid,
        input: &CreateCameraHealthLog,
    ) -> Result<CameraHealthLog, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, CameraHealthLog>(
            r#"
            INSERT INTO camera_health_logs
                (tenant_id, camera_id, alive, latency_ms, error, source_device_id)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(tenant_id)
        .bind(camera_id)
        .bind(input.alive)
        .bind(input.latency_ms)
        .bind(input.error.as_ref())
        .bind(input.source_device_id.as_ref())
        .fetch_one(&mut *tc.conn)
        .await
    }

    async fn recent_health_logs(
        &self,
        tenant_id: Uuid,
        camera_id: Uuid,
        limit: i64,
    ) -> Result<Vec<CameraHealthLog>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, CameraHealthLog>(
            r#"
            SELECT * FROM camera_health_logs
            WHERE camera_id = $1
            ORDER BY checked_at DESC
            LIMIT $2
            "#,
        )
        .bind(camera_id)
        .bind(limit)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn statuses(&self, tenant_id: Uuid) -> Result<Vec<CameraStatusRow>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        // 各カメラの最新ログを DISTINCT ON で 1 件取り、マスタと LEFT JOIN する。
        sqlx::query_as::<_, CameraStatusRow>(
            r#"
            SELECT
                c.id AS camera_id,
                c.name AS name,
                c.active AS active,
                l.alive AS last_alive,
                l.latency_ms AS last_latency_ms,
                l.checked_at AS last_checked_at,
                c.active_down_ticket_id AS active_down_ticket_id
            FROM cameras c
            LEFT JOIN LATERAL (
                SELECT alive, latency_ms, checked_at
                FROM camera_health_logs hl
                WHERE hl.camera_id = c.id
                ORDER BY hl.checked_at DESC
                LIMIT 1
            ) l ON true
            ORDER BY c.name, c.created_at
            "#,
        )
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn set_active_down_ticket(
        &self,
        tenant_id: Uuid,
        camera_id: Uuid,
        ticket_id: Option<Uuid>,
    ) -> Result<(), sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query(
            "UPDATE cameras SET active_down_ticket_id = $2, updated_at = now() WHERE id = $1",
        )
        .bind(camera_id)
        .bind(ticket_id)
        .execute(&mut *tc.conn)
        .await?;
        Ok(())
    }
}

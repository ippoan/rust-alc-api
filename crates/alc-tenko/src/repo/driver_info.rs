use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use alc_core::models::{CarryingItem, DtakoDailyWorkHours, Employee};

use crate::models::{EmployeeHealthBaseline, EquipmentFailure, TenkoRecord};

use alc_core::tenant::TenantConn;

pub use crate::repository::driver_info::*;

pub struct PgDriverInfoRepository {
    pool: PgPool,
}

impl PgDriverInfoRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DriverInfoRepository for PgDriverInfoRepository {
    async fn get_employee(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Option<Employee>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, Employee>(
            "SELECT * FROM alc_api.employees WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(employee_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn get_health_baseline(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Option<EmployeeHealthBaseline>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, EmployeeHealthBaseline>(
            "SELECT * FROM alc_api.employee_health_baselines WHERE employee_id = $1",
        )
        .bind(employee_id)
        .fetch_optional(&mut *tc.conn)
        .await
    }

    async fn get_recent_measurements(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<MeasurementSummary>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, MeasurementSummary>(
            r#"SELECT id, temperature, systolic, diastolic, pulse, medical_measured_at AS measured_at
               FROM alc_api.tenko_sessions
               WHERE employee_id = $1 AND medical_measured_at IS NOT NULL
               ORDER BY medical_measured_at DESC LIMIT 5"#,
        )
        .bind(employee_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get_working_hours(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<DtakoDailyWorkHours>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, DtakoDailyWorkHours>(
            r#"SELECT * FROM alc_api.dtako_daily_work_hours
               WHERE driver_id = $1
               ORDER BY work_date DESC LIMIT 7"#,
        )
        .bind(employee_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get_past_instructions(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<InstructionSummary>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, InstructionSummary>(
            r#"SELECT session_id, instruction, instruction_confirmed_at, recorded_at
               FROM alc_api.tenko_records
               WHERE employee_id = $1 AND instruction IS NOT NULL AND instruction != ''
               ORDER BY recorded_at DESC LIMIT 10"#,
        )
        .bind(employee_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get_carrying_items(&self, tenant_id: Uuid) -> Result<Vec<CarryingItem>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, CarryingItem>(
            "SELECT * FROM alc_api.carrying_items ORDER BY sort_order, created_at",
        )
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get_past_tenko_records(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<TenkoRecord>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, TenkoRecord>(
            r#"SELECT * FROM alc_api.tenko_records
               WHERE employee_id = $1
               ORDER BY recorded_at DESC LIMIT 10"#,
        )
        .bind(employee_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get_recent_daily_inspections(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<DailyInspectionSummary>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, DailyInspectionSummary>(
            r#"SELECT session_id, daily_inspection, recorded_at
               FROM alc_api.tenko_records
               WHERE employee_id = $1 AND daily_inspection IS NOT NULL
               ORDER BY recorded_at DESC LIMIT 5"#,
        )
        .bind(employee_id)
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get_equipment_failures(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<EquipmentFailure>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        sqlx::query_as::<_, EquipmentFailure>(
            r#"SELECT * FROM alc_api.equipment_failures
               WHERE resolved_at IS NULL
               ORDER BY reported_at DESC"#,
        )
        .fetch_all(&mut *tc.conn)
        .await
    }

    async fn get_recent_maintenance_records(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
    ) -> Result<Vec<MaintenanceRecordSummary>, sqlx::Error> {
        let mut tc = TenantConn::acquire(&self.pool, &tenant_id.to_string()).await?;
        // 直近の点呼セッションの carins_vehicle_id → maintenance_vehicles.car_id →
        // maintenance_records を 1 CTE で解決する (N+1 にしない)。`alc-tenko` と
        // `alc-maintenance` は兄弟 crate で直接依存しない規約 (crates/alc-maintenance/
        // src/lib.rs) のため、ここは alc-maintenance に頼らず生 SQL で直接引く
        // (前例: crates/alc-maintenance/src/vehicles.rs の carins_candidates が
        // 他ドメインの car_inspection を crate 依存なしで叩いている)。
        //
        // carins_vehicle_id が NULL/空文字、または一致する maintenance_vehicles が
        // 無ければ latest_session.car_id / vehicle が空になり、結果も自然に空配列に
        // なる (404/500 にしない。carins を紐づけていないテナントでは解決しないのが
        // 仕様)。空文字列ガードは crates/alc-maintenance/src/vehicles.rs の
        // carins_candidates と同じ作法 (NULLIF で '' を NULL 扱いにする)。
        // RLS だけに任せず WHERE 句にも tenant_id を明示する (この repo の作法)。
        sqlx::query_as::<_, MaintenanceRecordSummary>(
            r#"WITH latest_session AS (
                   SELECT NULLIF(carins_vehicle_id, '') AS car_id
                   FROM alc_api.tenko_sessions
                   WHERE tenant_id = $1 AND employee_id = $2
                   ORDER BY created_at DESC
                   LIMIT 1
               ),
               vehicle AS (
                   SELECT mv.id
                   FROM alc_api.maintenance_vehicles mv, latest_session ls
                   WHERE mv.tenant_id = $1
                     AND mv.car_id = ls.car_id
                     AND mv.deleted_at IS NULL
               )
               SELECT mr.id, mr.vehicle_id, mc.name AS category_name, mr.performed_on,
                      mr.odometer_km, mr.vendor, mr.description, mr.cost::text AS cost,
                      mr.next_due_on
               FROM alc_api.maintenance_records mr
               JOIN alc_api.maintenance_categories mc
                   ON mc.id = mr.category_id AND mc.tenant_id = $1
               JOIN vehicle v ON v.id = mr.vehicle_id
               WHERE mr.tenant_id = $1 AND mr.deleted_at IS NULL
               ORDER BY mr.performed_on DESC
               LIMIT 5"#,
        )
        .bind(tenant_id)
        .bind(employee_id)
        .fetch_all(&mut *tc.conn)
        .await
    }
}

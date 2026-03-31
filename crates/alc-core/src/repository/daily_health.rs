use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DailyHealthRow {
    pub employee_id: Uuid,
    pub employee_name: String,
    pub employee_code: Option<String>,
    // Session
    pub session_id: Option<Uuid>,
    pub tenko_type: Option<String>,
    pub completed_at: Option<DateTime<Utc>>,
    // Medical
    pub temperature: Option<f64>,
    pub systolic: Option<i32>,
    pub diastolic: Option<i32>,
    pub pulse: Option<i32>,
    pub medical_measured_at: Option<DateTime<Utc>>,
    pub medical_manual_input: Option<bool>,
    // Alcohol
    pub alcohol_result: Option<String>,
    pub alcohol_value: Option<f64>,
    // JSONB
    pub self_declaration: Option<serde_json::Value>,
    pub safety_judgment: Option<serde_json::Value>,
    // Baseline
    pub has_baseline: Option<bool>,
    pub baseline_systolic: Option<i32>,
    pub baseline_diastolic: Option<i32>,
    pub baseline_temperature: Option<f64>,
    pub systolic_tolerance: Option<i32>,
    pub diastolic_tolerance: Option<i32>,
    pub temperature_tolerance: Option<f64>,
}

#[async_trait]
pub trait DailyHealthRepository: Send + Sync {
    async fn fetch_daily_health(
        &self,
        tenant_id: Uuid,
        date: NaiveDate,
    ) -> Result<Vec<DailyHealthRow>, sqlx::Error>;
}

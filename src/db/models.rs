use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// --- Tenant ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Tenant {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

// --- Employee ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Employee {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub nfc_id: String,
    pub name: String,
    pub face_photo_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateEmployee {
    pub nfc_id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateFace {
    pub face_photo_url: String,
}

// --- User ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub google_sub: String,
    pub email: String,
    pub name: String,
    pub role: String,
    pub refresh_token_hash: Option<String>,
    pub refresh_token_expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

// --- Measurement ---

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Measurement {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub employee_id: Uuid,
    pub alcohol_level: f64,
    pub result: String, // "pass" | "fail"
    pub face_photo_url: Option<String>,
    pub measured_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateMeasurement {
    pub employee_id: Uuid,
    pub alcohol_level: f64,
    pub result: String,
    pub face_photo_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MeasurementFilter {
    pub employee_id: Option<Uuid>,
    pub result: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

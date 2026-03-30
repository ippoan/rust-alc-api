use uuid::Uuid;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use serde_json::Value;

use rust_alc_api::db::models::TimecardCard;
use rust_alc_api::db::repository::timecard::{TimePunchCsvRow, TimecardRepository};

// ============================================================
// Helper: spawn server with a given TimecardRepository mock
// ============================================================

async fn spawn_with_mock(mock: Arc<dyn TimecardRepository>) -> (String, String) {
    let tenant_id = Uuid::new_v4();
    let jwt = crate::common::create_test_jwt(tenant_id, "admin");

    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.timecard = mock;
    let base_url = crate::common::spawn_test_server(state).await;

    (base_url, jwt)
}

fn auth(jwt: &str) -> String {
    format!("Bearer {jwt}")
}

fn make_card(tenant_id: Uuid, employee_id: Uuid, card_id: &str) -> TimecardCard {
    TimecardCard {
        id: Uuid::new_v4(),
        tenant_id,
        employee_id,
        card_id: card_id.to_string(),
        label: Some("Test Card".to_string()),
        created_at: Utc::now(),
    }
}

// ============================================================
// Custom mock: punch with card found (find_card_by_card_id returns Some)
// ============================================================

struct PunchCardFoundMock {
    employee_id: Uuid,
}

#[async_trait::async_trait]
impl TimecardRepository for PunchCardFoundMock {
    async fn create_card(
        &self,
        _: Uuid,
        _: Uuid,
        _: &str,
        _: Option<&str>,
    ) -> Result<TimecardCard, sqlx::Error> {
        unreachable!()
    }
    async fn list_cards(&self, _: Uuid, _: Option<Uuid>) -> Result<Vec<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn get_card(&self, _: Uuid, _: Uuid) -> Result<Option<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn get_card_by_card_id(
        &self,
        _: Uuid,
        _: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn delete_card(&self, _: Uuid, _: Uuid) -> Result<bool, sqlx::Error> {
        unreachable!()
    }

    async fn find_card_by_card_id(
        &self,
        tenant_id: Uuid,
        card_id: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        Ok(Some(TimecardCard {
            id: Uuid::new_v4(),
            tenant_id,
            employee_id: self.employee_id,
            card_id: card_id.to_string(),
            label: None,
            created_at: Utc::now(),
        }))
    }

    async fn find_employee_id_by_nfc(&self, _: Uuid, _: &str) -> Result<Option<Uuid>, sqlx::Error> {
        // Should not be called when find_card_by_card_id returns Some
        unreachable!("NFC fallback should not be called when card is found")
    }

    async fn create_punch(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        device_id: Option<Uuid>,
    ) -> Result<rust_alc_api::db::models::TimePunch, sqlx::Error> {
        Ok(rust_alc_api::db::models::TimePunch {
            id: Uuid::new_v4(),
            tenant_id,
            employee_id,
            device_id,
            punched_at: Utc::now(),
            created_at: Utc::now(),
        })
    }

    async fn get_employee_name(&self, _: Uuid, _: Uuid) -> Result<String, sqlx::Error> {
        Ok("Taro Yamada".to_string())
    }

    async fn list_today_punches(
        &self,
        _: Uuid,
        _: Uuid,
    ) -> Result<Vec<rust_alc_api::db::models::TimePunch>, sqlx::Error> {
        Ok(vec![])
    }

    async fn count_punches(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: Option<chrono::DateTime<Utc>>,
        _: Option<chrono::DateTime<Utc>>,
    ) -> Result<i64, sqlx::Error> {
        unreachable!()
    }
    async fn list_punches(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: Option<chrono::DateTime<Utc>>,
        _: Option<chrono::DateTime<Utc>>,
        _: i64,
        _: i64,
    ) -> Result<Vec<rust_alc_api::db::models::TimePunchWithDevice>, sqlx::Error> {
        unreachable!()
    }
    async fn list_punches_for_csv(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: Option<chrono::DateTime<Utc>>,
        _: Option<chrono::DateTime<Utc>>,
    ) -> Result<Vec<TimePunchCsvRow>, sqlx::Error> {
        unreachable!()
    }
}

// ============================================================
// Custom mock: punch NFC fallback (find_card_by_card_id returns None, NFC returns Some)
// ============================================================

struct PunchNfcFallbackMock {
    employee_id: Uuid,
}

#[async_trait::async_trait]
impl TimecardRepository for PunchNfcFallbackMock {
    async fn create_card(
        &self,
        _: Uuid,
        _: Uuid,
        _: &str,
        _: Option<&str>,
    ) -> Result<TimecardCard, sqlx::Error> {
        unreachable!()
    }
    async fn list_cards(&self, _: Uuid, _: Option<Uuid>) -> Result<Vec<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn get_card(&self, _: Uuid, _: Uuid) -> Result<Option<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn get_card_by_card_id(
        &self,
        _: Uuid,
        _: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn delete_card(&self, _: Uuid, _: Uuid) -> Result<bool, sqlx::Error> {
        unreachable!()
    }

    async fn find_card_by_card_id(
        &self,
        _: Uuid,
        _: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        Ok(None) // Card not found -> triggers NFC fallback
    }

    async fn find_employee_id_by_nfc(&self, _: Uuid, _: &str) -> Result<Option<Uuid>, sqlx::Error> {
        Ok(Some(self.employee_id))
    }

    async fn create_punch(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        device_id: Option<Uuid>,
    ) -> Result<rust_alc_api::db::models::TimePunch, sqlx::Error> {
        Ok(rust_alc_api::db::models::TimePunch {
            id: Uuid::new_v4(),
            tenant_id,
            employee_id,
            device_id,
            punched_at: Utc::now(),
            created_at: Utc::now(),
        })
    }

    async fn get_employee_name(&self, _: Uuid, _: Uuid) -> Result<String, sqlx::Error> {
        Ok("Hanako Tanaka".to_string())
    }

    async fn list_today_punches(
        &self,
        _: Uuid,
        _: Uuid,
    ) -> Result<Vec<rust_alc_api::db::models::TimePunch>, sqlx::Error> {
        Ok(vec![])
    }

    async fn count_punches(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: Option<chrono::DateTime<Utc>>,
        _: Option<chrono::DateTime<Utc>>,
    ) -> Result<i64, sqlx::Error> {
        unreachable!()
    }
    async fn list_punches(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: Option<chrono::DateTime<Utc>>,
        _: Option<chrono::DateTime<Utc>>,
        _: i64,
        _: i64,
    ) -> Result<Vec<rust_alc_api::db::models::TimePunchWithDevice>, sqlx::Error> {
        unreachable!()
    }
    async fn list_punches_for_csv(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: Option<chrono::DateTime<Utc>>,
        _: Option<chrono::DateTime<Utc>>,
    ) -> Result<Vec<TimePunchCsvRow>, sqlx::Error> {
        unreachable!()
    }
}

// ============================================================
// Custom mock: punch create_punch fails (for DB error after employee found)
// ============================================================

struct PunchCreateFailMock;

#[async_trait::async_trait]
impl TimecardRepository for PunchCreateFailMock {
    async fn create_card(
        &self,
        _: Uuid,
        _: Uuid,
        _: &str,
        _: Option<&str>,
    ) -> Result<TimecardCard, sqlx::Error> {
        unreachable!()
    }
    async fn list_cards(&self, _: Uuid, _: Option<Uuid>) -> Result<Vec<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn get_card(&self, _: Uuid, _: Uuid) -> Result<Option<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn get_card_by_card_id(
        &self,
        _: Uuid,
        _: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn delete_card(&self, _: Uuid, _: Uuid) -> Result<bool, sqlx::Error> {
        unreachable!()
    }

    async fn find_card_by_card_id(
        &self,
        tenant_id: Uuid,
        card_id: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        Ok(Some(TimecardCard {
            id: Uuid::new_v4(),
            tenant_id,
            employee_id: Uuid::new_v4(),
            card_id: card_id.to_string(),
            label: None,
            created_at: Utc::now(),
        }))
    }

    async fn find_employee_id_by_nfc(&self, _: Uuid, _: &str) -> Result<Option<Uuid>, sqlx::Error> {
        unreachable!()
    }

    async fn create_punch(
        &self,
        _: Uuid,
        _: Uuid,
        _: Option<Uuid>,
    ) -> Result<rust_alc_api::db::models::TimePunch, sqlx::Error> {
        Err(sqlx::Error::RowNotFound)
    }

    async fn get_employee_name(&self, _: Uuid, _: Uuid) -> Result<String, sqlx::Error> {
        unreachable!()
    }
    async fn list_today_punches(
        &self,
        _: Uuid,
        _: Uuid,
    ) -> Result<Vec<rust_alc_api::db::models::TimePunch>, sqlx::Error> {
        unreachable!()
    }
    async fn count_punches(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: Option<chrono::DateTime<Utc>>,
        _: Option<chrono::DateTime<Utc>>,
    ) -> Result<i64, sqlx::Error> {
        unreachable!()
    }
    async fn list_punches(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: Option<chrono::DateTime<Utc>>,
        _: Option<chrono::DateTime<Utc>>,
        _: i64,
        _: i64,
    ) -> Result<Vec<rust_alc_api::db::models::TimePunchWithDevice>, sqlx::Error> {
        unreachable!()
    }
    async fn list_punches_for_csv(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: Option<chrono::DateTime<Utc>>,
        _: Option<chrono::DateTime<Utc>>,
    ) -> Result<Vec<TimePunchCsvRow>, sqlx::Error> {
        unreachable!()
    }
}

// ============================================================
// Custom mock: punch get_employee_name fails
// ============================================================

struct PunchGetNameFailMock;

#[async_trait::async_trait]
impl TimecardRepository for PunchGetNameFailMock {
    async fn create_card(
        &self,
        _: Uuid,
        _: Uuid,
        _: &str,
        _: Option<&str>,
    ) -> Result<TimecardCard, sqlx::Error> {
        unreachable!()
    }
    async fn list_cards(&self, _: Uuid, _: Option<Uuid>) -> Result<Vec<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn get_card(&self, _: Uuid, _: Uuid) -> Result<Option<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn get_card_by_card_id(
        &self,
        _: Uuid,
        _: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn delete_card(&self, _: Uuid, _: Uuid) -> Result<bool, sqlx::Error> {
        unreachable!()
    }

    async fn find_card_by_card_id(
        &self,
        tenant_id: Uuid,
        card_id: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        Ok(Some(TimecardCard {
            id: Uuid::new_v4(),
            tenant_id,
            employee_id: Uuid::new_v4(),
            card_id: card_id.to_string(),
            label: None,
            created_at: Utc::now(),
        }))
    }

    async fn find_employee_id_by_nfc(&self, _: Uuid, _: &str) -> Result<Option<Uuid>, sqlx::Error> {
        unreachable!()
    }

    async fn create_punch(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        device_id: Option<Uuid>,
    ) -> Result<rust_alc_api::db::models::TimePunch, sqlx::Error> {
        Ok(rust_alc_api::db::models::TimePunch {
            id: Uuid::new_v4(),
            tenant_id,
            employee_id,
            device_id,
            punched_at: Utc::now(),
            created_at: Utc::now(),
        })
    }

    async fn get_employee_name(&self, _: Uuid, _: Uuid) -> Result<String, sqlx::Error> {
        Err(sqlx::Error::RowNotFound)
    }

    async fn list_today_punches(
        &self,
        _: Uuid,
        _: Uuid,
    ) -> Result<Vec<rust_alc_api::db::models::TimePunch>, sqlx::Error> {
        unreachable!()
    }
    async fn count_punches(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: Option<chrono::DateTime<Utc>>,
        _: Option<chrono::DateTime<Utc>>,
    ) -> Result<i64, sqlx::Error> {
        unreachable!()
    }
    async fn list_punches(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: Option<chrono::DateTime<Utc>>,
        _: Option<chrono::DateTime<Utc>>,
        _: i64,
        _: i64,
    ) -> Result<Vec<rust_alc_api::db::models::TimePunchWithDevice>, sqlx::Error> {
        unreachable!()
    }
    async fn list_punches_for_csv(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: Option<chrono::DateTime<Utc>>,
        _: Option<chrono::DateTime<Utc>>,
    ) -> Result<Vec<TimePunchCsvRow>, sqlx::Error> {
        unreachable!()
    }
}

// ============================================================
// Custom mock: punch list_today_punches fails
// ============================================================

struct PunchListTodayFailMock;

#[async_trait::async_trait]
impl TimecardRepository for PunchListTodayFailMock {
    async fn create_card(
        &self,
        _: Uuid,
        _: Uuid,
        _: &str,
        _: Option<&str>,
    ) -> Result<TimecardCard, sqlx::Error> {
        unreachable!()
    }
    async fn list_cards(&self, _: Uuid, _: Option<Uuid>) -> Result<Vec<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn get_card(&self, _: Uuid, _: Uuid) -> Result<Option<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn get_card_by_card_id(
        &self,
        _: Uuid,
        _: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn delete_card(&self, _: Uuid, _: Uuid) -> Result<bool, sqlx::Error> {
        unreachable!()
    }

    async fn find_card_by_card_id(
        &self,
        tenant_id: Uuid,
        card_id: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        Ok(Some(TimecardCard {
            id: Uuid::new_v4(),
            tenant_id,
            employee_id: Uuid::new_v4(),
            card_id: card_id.to_string(),
            label: None,
            created_at: Utc::now(),
        }))
    }

    async fn find_employee_id_by_nfc(&self, _: Uuid, _: &str) -> Result<Option<Uuid>, sqlx::Error> {
        unreachable!()
    }

    async fn create_punch(
        &self,
        tenant_id: Uuid,
        employee_id: Uuid,
        device_id: Option<Uuid>,
    ) -> Result<rust_alc_api::db::models::TimePunch, sqlx::Error> {
        Ok(rust_alc_api::db::models::TimePunch {
            id: Uuid::new_v4(),
            tenant_id,
            employee_id,
            device_id,
            punched_at: Utc::now(),
            created_at: Utc::now(),
        })
    }

    async fn get_employee_name(&self, _: Uuid, _: Uuid) -> Result<String, sqlx::Error> {
        Ok("Test Employee".to_string())
    }

    async fn list_today_punches(
        &self,
        _: Uuid,
        _: Uuid,
    ) -> Result<Vec<rust_alc_api::db::models::TimePunch>, sqlx::Error> {
        Err(sqlx::Error::RowNotFound)
    }

    async fn count_punches(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: Option<chrono::DateTime<Utc>>,
        _: Option<chrono::DateTime<Utc>>,
    ) -> Result<i64, sqlx::Error> {
        unreachable!()
    }
    async fn list_punches(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: Option<chrono::DateTime<Utc>>,
        _: Option<chrono::DateTime<Utc>>,
        _: i64,
        _: i64,
    ) -> Result<Vec<rust_alc_api::db::models::TimePunchWithDevice>, sqlx::Error> {
        unreachable!()
    }
    async fn list_punches_for_csv(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: Option<chrono::DateTime<Utc>>,
        _: Option<chrono::DateTime<Utc>>,
    ) -> Result<Vec<TimePunchCsvRow>, sqlx::Error> {
        unreachable!()
    }
}

// ============================================================
// Custom mock: punch find_card_by_card_id DB error
// ============================================================

struct PunchFindCardDbErrorMock;

#[async_trait::async_trait]
impl TimecardRepository for PunchFindCardDbErrorMock {
    async fn create_card(
        &self,
        _: Uuid,
        _: Uuid,
        _: &str,
        _: Option<&str>,
    ) -> Result<TimecardCard, sqlx::Error> {
        unreachable!()
    }
    async fn list_cards(&self, _: Uuid, _: Option<Uuid>) -> Result<Vec<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn get_card(&self, _: Uuid, _: Uuid) -> Result<Option<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn get_card_by_card_id(
        &self,
        _: Uuid,
        _: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn delete_card(&self, _: Uuid, _: Uuid) -> Result<bool, sqlx::Error> {
        unreachable!()
    }

    async fn find_card_by_card_id(
        &self,
        _: Uuid,
        _: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        Err(sqlx::Error::RowNotFound)
    }

    async fn find_employee_id_by_nfc(&self, _: Uuid, _: &str) -> Result<Option<Uuid>, sqlx::Error> {
        unreachable!()
    }

    async fn create_punch(
        &self,
        _: Uuid,
        _: Uuid,
        _: Option<Uuid>,
    ) -> Result<rust_alc_api::db::models::TimePunch, sqlx::Error> {
        unreachable!()
    }
    async fn get_employee_name(&self, _: Uuid, _: Uuid) -> Result<String, sqlx::Error> {
        unreachable!()
    }
    async fn list_today_punches(
        &self,
        _: Uuid,
        _: Uuid,
    ) -> Result<Vec<rust_alc_api::db::models::TimePunch>, sqlx::Error> {
        unreachable!()
    }
    async fn count_punches(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: Option<chrono::DateTime<Utc>>,
        _: Option<chrono::DateTime<Utc>>,
    ) -> Result<i64, sqlx::Error> {
        unreachable!()
    }
    async fn list_punches(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: Option<chrono::DateTime<Utc>>,
        _: Option<chrono::DateTime<Utc>>,
        _: i64,
        _: i64,
    ) -> Result<Vec<rust_alc_api::db::models::TimePunchWithDevice>, sqlx::Error> {
        unreachable!()
    }
    async fn list_punches_for_csv(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: Option<chrono::DateTime<Utc>>,
        _: Option<chrono::DateTime<Utc>>,
    ) -> Result<Vec<TimePunchCsvRow>, sqlx::Error> {
        unreachable!()
    }
}

// ============================================================
// Custom mock: punch NFC fallback DB error
// ============================================================

struct PunchNfcDbErrorMock;

#[async_trait::async_trait]
impl TimecardRepository for PunchNfcDbErrorMock {
    async fn create_card(
        &self,
        _: Uuid,
        _: Uuid,
        _: &str,
        _: Option<&str>,
    ) -> Result<TimecardCard, sqlx::Error> {
        unreachable!()
    }
    async fn list_cards(&self, _: Uuid, _: Option<Uuid>) -> Result<Vec<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn get_card(&self, _: Uuid, _: Uuid) -> Result<Option<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn get_card_by_card_id(
        &self,
        _: Uuid,
        _: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn delete_card(&self, _: Uuid, _: Uuid) -> Result<bool, sqlx::Error> {
        unreachable!()
    }

    async fn find_card_by_card_id(
        &self,
        _: Uuid,
        _: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        Ok(None) // Card not found -> triggers NFC fallback
    }

    async fn find_employee_id_by_nfc(&self, _: Uuid, _: &str) -> Result<Option<Uuid>, sqlx::Error> {
        Err(sqlx::Error::RowNotFound) // NFC lookup fails with DB error
    }

    async fn create_punch(
        &self,
        _: Uuid,
        _: Uuid,
        _: Option<Uuid>,
    ) -> Result<rust_alc_api::db::models::TimePunch, sqlx::Error> {
        unreachable!()
    }
    async fn get_employee_name(&self, _: Uuid, _: Uuid) -> Result<String, sqlx::Error> {
        unreachable!()
    }
    async fn list_today_punches(
        &self,
        _: Uuid,
        _: Uuid,
    ) -> Result<Vec<rust_alc_api::db::models::TimePunch>, sqlx::Error> {
        unreachable!()
    }
    async fn count_punches(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: Option<chrono::DateTime<Utc>>,
        _: Option<chrono::DateTime<Utc>>,
    ) -> Result<i64, sqlx::Error> {
        unreachable!()
    }
    async fn list_punches(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: Option<chrono::DateTime<Utc>>,
        _: Option<chrono::DateTime<Utc>>,
        _: i64,
        _: i64,
    ) -> Result<Vec<rust_alc_api::db::models::TimePunchWithDevice>, sqlx::Error> {
        unreachable!()
    }
    async fn list_punches_for_csv(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: Option<chrono::DateTime<Utc>>,
        _: Option<chrono::DateTime<Utc>>,
    ) -> Result<Vec<TimePunchCsvRow>, sqlx::Error> {
        unreachable!()
    }
}

// ============================================================
// Custom mock: list_punches count_punches fails
// ============================================================

struct ListPunchesCountFailMock;

#[async_trait::async_trait]
impl TimecardRepository for ListPunchesCountFailMock {
    async fn create_card(
        &self,
        _: Uuid,
        _: Uuid,
        _: &str,
        _: Option<&str>,
    ) -> Result<TimecardCard, sqlx::Error> {
        unreachable!()
    }
    async fn list_cards(&self, _: Uuid, _: Option<Uuid>) -> Result<Vec<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn get_card(&self, _: Uuid, _: Uuid) -> Result<Option<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn get_card_by_card_id(
        &self,
        _: Uuid,
        _: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn delete_card(&self, _: Uuid, _: Uuid) -> Result<bool, sqlx::Error> {
        unreachable!()
    }
    async fn find_card_by_card_id(
        &self,
        _: Uuid,
        _: &str,
    ) -> Result<Option<TimecardCard>, sqlx::Error> {
        unreachable!()
    }
    async fn find_employee_id_by_nfc(&self, _: Uuid, _: &str) -> Result<Option<Uuid>, sqlx::Error> {
        unreachable!()
    }
    async fn create_punch(
        &self,
        _: Uuid,
        _: Uuid,
        _: Option<Uuid>,
    ) -> Result<rust_alc_api::db::models::TimePunch, sqlx::Error> {
        unreachable!()
    }
    async fn get_employee_name(&self, _: Uuid, _: Uuid) -> Result<String, sqlx::Error> {
        unreachable!()
    }
    async fn list_today_punches(
        &self,
        _: Uuid,
        _: Uuid,
    ) -> Result<Vec<rust_alc_api::db::models::TimePunch>, sqlx::Error> {
        unreachable!()
    }

    async fn count_punches(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: Option<chrono::DateTime<Utc>>,
        _: Option<chrono::DateTime<Utc>>,
    ) -> Result<i64, sqlx::Error> {
        Err(sqlx::Error::RowNotFound)
    }

    async fn list_punches(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: Option<chrono::DateTime<Utc>>,
        _: Option<chrono::DateTime<Utc>>,
        _: i64,
        _: i64,
    ) -> Result<Vec<rust_alc_api::db::models::TimePunchWithDevice>, sqlx::Error> {
        unreachable!()
    }
    async fn list_punches_for_csv(
        &self,
        _: Uuid,
        _: Option<Uuid>,
        _: Option<chrono::DateTime<Utc>>,
        _: Option<chrono::DateTime<Utc>>,
    ) -> Result<Vec<TimePunchCsvRow>, sqlx::Error> {
        unreachable!()
    }
}

// ============================================================
// POST /api/timecard/cards — create_card
// ============================================================

#[tokio::test]
async fn test_create_card_success() {
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let employee_id = Uuid::new_v4();
    let res = client
        .post(format!("{base_url}/api/timecard/cards"))
        .header("Authorization", auth(&jwt))
        .json(&serde_json::json!({
            "employee_id": employee_id,
            "card_id": "NFC-001",
            "label": "Main Card"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);

    let body: Value = res.json().await.unwrap();
    assert_eq!(body["card_id"], "NFC-001");
    assert_eq!(body["label"], "Main Card");
    assert_eq!(body["employee_id"], employee_id.to_string());
}

#[tokio::test]
async fn test_create_card_without_label() {
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/timecard/cards"))
        .header("Authorization", auth(&jwt))
        .json(&serde_json::json!({
            "employee_id": Uuid::new_v4(),
            "card_id": "NFC-002"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);

    let body: Value = res.json().await.unwrap();
    assert!(body["label"].is_null());
}

#[tokio::test]
async fn test_create_card_conflict() {
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    mock.create_card_conflict.store(true, Ordering::SeqCst);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/timecard/cards"))
        .header("Authorization", auth(&jwt))
        .json(&serde_json::json!({
            "employee_id": Uuid::new_v4(),
            "card_id": "DUPLICATE-CARD"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 409);
}

#[tokio::test]
async fn test_create_card_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/timecard/cards"))
        .header("Authorization", auth(&jwt))
        .json(&serde_json::json!({
            "employee_id": Uuid::new_v4(),
            "card_id": "NFC-ERR"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// GET /api/timecard/cards — list_cards
// ============================================================

#[tokio::test]
async fn test_list_cards_success_empty() {
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/timecard/cards"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Vec<Value> = res.json().await.unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn test_list_cards_with_employee_id_filter() {
    let tenant_id = Uuid::new_v4();
    let employee_id = Uuid::new_v4();
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    *mock.cards_list.lock().unwrap() = vec![make_card(tenant_id, employee_id, "CARD-A")];

    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!(
            "{base_url}/api/timecard/cards?employee_id={employee_id}"
        ))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Vec<Value> = res.json().await.unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["card_id"], "CARD-A");
}

#[tokio::test]
async fn test_list_cards_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/timecard/cards"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// GET /api/timecard/cards/{id} — get_card
// ============================================================

#[tokio::test]
async fn test_get_card_found() {
    let tenant_id = Uuid::new_v4();
    let employee_id = Uuid::new_v4();
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    *mock.card_data.lock().unwrap() = Some(make_card(tenant_id, employee_id, "CARD-X"));

    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let card_id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/timecard/cards/{card_id}"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    assert_eq!(body["card_id"], "CARD-X");
}

#[tokio::test]
async fn test_get_card_not_found() {
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    // Default card_data is None
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let card_id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/timecard/cards/{card_id}"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_get_card_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let card_id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/timecard/cards/{card_id}"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// GET /api/timecard/cards/by-card/{card_id} — get_card_by_card_id
// ============================================================

#[tokio::test]
async fn test_get_card_by_card_id_found() {
    let tenant_id = Uuid::new_v4();
    let employee_id = Uuid::new_v4();
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    *mock.card_data.lock().unwrap() = Some(make_card(tenant_id, employee_id, "NFC-FIND"));

    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/timecard/cards/by-card/NFC-FIND"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    assert_eq!(body["card_id"], "NFC-FIND");
}

#[tokio::test]
async fn test_get_card_by_card_id_not_found() {
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/timecard/cards/by-card/NONEXISTENT"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_get_card_by_card_id_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/timecard/cards/by-card/SOME-CARD"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// DELETE /api/timecard/cards/{id} — delete_card
// ============================================================

#[tokio::test]
async fn test_delete_card_success() {
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    mock.delete_returns_true.store(true, Ordering::SeqCst);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let card_id = Uuid::new_v4();
    let res = client
        .delete(format!("{base_url}/api/timecard/cards/{card_id}"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 204);
}

#[tokio::test]
async fn test_delete_card_not_found() {
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    // Default delete_returns_true is false -> not found
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let card_id = Uuid::new_v4();
    let res = client
        .delete(format!("{base_url}/api/timecard/cards/{card_id}"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_delete_card_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let card_id = Uuid::new_v4();
    let res = client
        .delete(format!("{base_url}/api/timecard/cards/{card_id}"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// POST /api/timecard/punch — punch (card found path)
// ============================================================

#[tokio::test]
async fn test_punch_success_card_found() {
    let employee_id = Uuid::new_v4();
    let mock = Arc::new(PunchCardFoundMock { employee_id });
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/timecard/punch"))
        .header("Authorization", auth(&jwt))
        .json(&serde_json::json!({
            "card_id": "NFC-CARD-001"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);

    let body: Value = res.json().await.unwrap();
    assert_eq!(body["employee_name"], "Taro Yamada");
    assert!(body["punch"]["id"].is_string());
    assert!(body["today_punches"].is_array());
}

// ============================================================
// POST /api/timecard/punch — punch (NFC fallback path)
// ============================================================

#[tokio::test]
async fn test_punch_success_nfc_fallback() {
    let employee_id = Uuid::new_v4();
    let mock = Arc::new(PunchNfcFallbackMock { employee_id });
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/timecard/punch"))
        .header("Authorization", auth(&jwt))
        .json(&serde_json::json!({
            "card_id": "NFC-TAG-XYZ"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 201);

    let body: Value = res.json().await.unwrap();
    assert_eq!(body["employee_name"], "Hanako Tanaka");
}

// ============================================================
// POST /api/timecard/punch — both lookups return None -> 404
// ============================================================

#[tokio::test]
async fn test_punch_both_not_found() {
    // Default mock: find_card_by_card_id returns None, find_employee_id_by_nfc returns None
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/timecard/punch"))
        .header("Authorization", auth(&jwt))
        .json(&serde_json::json!({
            "card_id": "UNKNOWN-CARD"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

// ============================================================
// POST /api/timecard/punch — find_card_by_card_id DB error
// ============================================================

#[tokio::test]
async fn test_punch_find_card_db_error() {
    let mock = Arc::new(PunchFindCardDbErrorMock);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/timecard/punch"))
        .header("Authorization", auth(&jwt))
        .json(&serde_json::json!({
            "card_id": "NFC-ERR"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// POST /api/timecard/punch — NFC fallback DB error
// ============================================================

#[tokio::test]
async fn test_punch_nfc_db_error() {
    let mock = Arc::new(PunchNfcDbErrorMock);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/timecard/punch"))
        .header("Authorization", auth(&jwt))
        .json(&serde_json::json!({
            "card_id": "NFC-ERR"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// POST /api/timecard/punch — create_punch DB error
// ============================================================

#[tokio::test]
async fn test_punch_create_punch_db_error() {
    let mock = Arc::new(PunchCreateFailMock);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/timecard/punch"))
        .header("Authorization", auth(&jwt))
        .json(&serde_json::json!({
            "card_id": "NFC-CARD-001"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// POST /api/timecard/punch — get_employee_name DB error
// ============================================================

#[tokio::test]
async fn test_punch_get_name_db_error() {
    let mock = Arc::new(PunchGetNameFailMock);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/timecard/punch"))
        .header("Authorization", auth(&jwt))
        .json(&serde_json::json!({
            "card_id": "NFC-CARD-001"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// POST /api/timecard/punch — list_today_punches DB error
// ============================================================

#[tokio::test]
async fn test_punch_list_today_db_error() {
    let mock = Arc::new(PunchListTodayFailMock);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{base_url}/api/timecard/punch"))
        .header("Authorization", auth(&jwt))
        .json(&serde_json::json!({
            "card_id": "NFC-CARD-001"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// GET /api/timecard/punches — list_punches
// ============================================================

#[tokio::test]
async fn test_list_punches_success_empty() {
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/timecard/punches"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    assert_eq!(body["punches"], serde_json::json!([]));
    assert_eq!(body["total"], 0);
    assert_eq!(body["page"], 1);
    assert_eq!(body["per_page"], 50);
}

#[tokio::test]
async fn test_list_punches_with_pagination() {
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!(
            "{base_url}/api/timecard/punches?page=2&per_page=10"
        ))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    assert_eq!(body["page"], 2);
    assert_eq!(body["per_page"], 10);
}

#[tokio::test]
async fn test_list_punches_per_page_capped_at_200() {
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/timecard/punches?per_page=999"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let body: Value = res.json().await.unwrap();
    assert_eq!(body["per_page"], 200);
}

#[tokio::test]
async fn test_list_punches_db_error_count() {
    let mock = Arc::new(ListPunchesCountFailMock);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/timecard/punches"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// GET /api/timecard/punches/csv — export_csv
// ============================================================

#[tokio::test]
async fn test_export_csv_success_empty() {
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/timecard/punches/csv"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    // Check Content-Type
    let content_type = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(content_type.contains("text/csv"));

    // Check Content-Disposition
    let disposition = res
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(disposition.contains("time_punches.csv"));

    let bytes = res.bytes().await.unwrap();
    // Check BOM prefix
    assert_eq!(&bytes[0..3], &[0xEF, 0xBB, 0xBF]);

    // Check CSV header after BOM
    let csv_content = std::str::from_utf8(&bytes[3..]).unwrap();
    assert!(csv_content.starts_with("ID,"));
    assert!(csv_content.contains("社員コード"));
    assert!(csv_content.contains("社員名"));
    assert!(csv_content.contains("打刻日時"));
    assert!(csv_content.contains("デバイス"));
}

#[tokio::test]
async fn test_export_csv_with_data_jst_timezone() {
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    // 2026-01-15 00:30:00 UTC = 2026-01-15 09:30:00 JST
    let utc_time = Utc.with_ymd_and_hms(2026, 1, 15, 0, 30, 0).unwrap();
    *mock.csv_rows.lock().unwrap() = vec![TimePunchCsvRow {
        id: Uuid::new_v4(),
        punched_at: utc_time,
        employee_name: "Taro Test".to_string(),
        employee_code: Some("EMP001".to_string()),
        device_name: Some("Kiosk-A".to_string()),
    }];

    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/timecard/punches/csv"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let bytes = res.bytes().await.unwrap();
    let csv_content = std::str::from_utf8(&bytes[3..]).unwrap();

    // Verify JST conversion: UTC 00:30 -> JST 09:30
    assert!(csv_content.contains("2026-01-15 09:30:00"));
    assert!(csv_content.contains("Taro Test"));
    assert!(csv_content.contains("EMP001"));
    assert!(csv_content.contains("Kiosk-A"));
}

#[tokio::test]
async fn test_export_csv_with_null_fields() {
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    let utc_time = Utc.with_ymd_and_hms(2026, 3, 1, 15, 0, 0).unwrap();
    *mock.csv_rows.lock().unwrap() = vec![TimePunchCsvRow {
        id: Uuid::new_v4(),
        punched_at: utc_time,
        employee_name: "No Code Employee".to_string(),
        employee_code: None,
        device_name: None,
    }];

    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/timecard/punches/csv"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    let bytes = res.bytes().await.unwrap();
    let csv_content = std::str::from_utf8(&bytes[3..]).unwrap();
    assert!(csv_content.contains("No Code Employee"));
    // JST: 15:00 UTC -> 00:00 JST next day (2026-03-02)
    assert!(csv_content.contains("2026-03-02 00:00:00"));
}

#[tokio::test]
async fn test_export_csv_db_error() {
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    mock.fail_next.store(true, Ordering::SeqCst);
    let (base_url, jwt) = spawn_with_mock(mock).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base_url}/api/timecard/punches/csv"))
        .header("Authorization", auth(&jwt))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 500);
}

// ============================================================
// Auth: no JWT -> 401
// ============================================================

#[tokio::test]
async fn test_no_auth_returns_401() {
    let mock = Arc::new(crate::mock_helpers::MockTimecardRepository::default());
    let mut state = crate::mock_helpers::app_state::setup_mock_app_state();
    state.timecard = mock;
    let base_url = crate::common::spawn_test_server(state).await;
    let client = reqwest::Client::new();

    // POST /timecard/cards
    let res = client
        .post(format!("{base_url}/api/timecard/cards"))
        .json(&serde_json::json!({
            "employee_id": Uuid::new_v4(),
            "card_id": "NFC-001"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // GET /timecard/cards
    let res = client
        .get(format!("{base_url}/api/timecard/cards"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // GET /timecard/cards/{id}
    let id = Uuid::new_v4();
    let res = client
        .get(format!("{base_url}/api/timecard/cards/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // GET /timecard/cards/by-card/{card_id}
    let res = client
        .get(format!("{base_url}/api/timecard/cards/by-card/NFC-001"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // DELETE /timecard/cards/{id}
    let res = client
        .delete(format!("{base_url}/api/timecard/cards/{id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // POST /timecard/punch
    let res = client
        .post(format!("{base_url}/api/timecard/punch"))
        .json(&serde_json::json!({
            "card_id": "NFC-001"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // GET /timecard/punches
    let res = client
        .get(format!("{base_url}/api/timecard/punches"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);

    // GET /timecard/punches/csv
    let res = client
        .get(format!("{base_url}/api/timecard/punches/csv"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

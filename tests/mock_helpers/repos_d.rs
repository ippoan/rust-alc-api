use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use uuid::Uuid;

use rust_alc_api::db::repository::notify_deliveries::*;
use rust_alc_api::db::repository::notify_documents::*;

macro_rules! check_fail {
    ($self:expr) => {
        if $self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
    };
}

// ============================================================
// MockNotifyDocumentRepository
// ============================================================

pub struct MockNotifyDocumentRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockNotifyDocumentRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

fn mock_document(tenant_id: Uuid) -> NotifyDocument {
    NotifyDocument {
        id: Uuid::new_v4(),
        tenant_id,
        source_type: "email".into(),
        source_sender: Some("test@example.com".into()),
        source_subject: Some("Test Document".into()),
        r2_key: "test/doc.pdf".into(),
        file_name: Some("doc.pdf".into()),
        file_size_bytes: Some(1024),
        extracted_title: None,
        extracted_date: None,
        extracted_summary: None,
        extracted_phone_numbers: None,
        extracted_data: None,
        extraction_status: "pending".into(),
        extraction_error: None,
        distribution_status: "pending".into(),
        distributed_at: None,
        redacted_r2_key: None,
        redacted_at: None,
        redactions_applied: None,
        redaction_status: "pending".into(),
        redaction_error: None,
        redact_dl_ms: None,
        redact_llm_ms: None,
        redact_render_ms: None,
        redact_upload_ms: None,
        redact_total_ms: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[async_trait::async_trait]
impl NotifyDocumentRepository for MockNotifyDocumentRepository {
    async fn create(
        &self,
        tenant_id: Uuid,
        _input: &CreateNotifyDocument,
    ) -> Result<NotifyDocument, sqlx::Error> {
        check_fail!(self);
        Ok(mock_document(tenant_id))
    }
    async fn get(&self, tenant_id: Uuid, _id: Uuid) -> Result<Option<NotifyDocument>, sqlx::Error> {
        check_fail!(self);
        Ok(Some(mock_document(tenant_id)))
    }
    async fn list(
        &self,
        tenant_id: Uuid,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<NotifyDocument>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![mock_document(tenant_id)])
    }
    async fn search(
        &self,
        tenant_id: Uuid,
        _query: &str,
    ) -> Result<Vec<NotifyDocument>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![mock_document(tenant_id)])
    }
    async fn update_extraction(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _result: &ExtractionResult,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }
    async fn update_extraction_error(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _error: &str,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }
    async fn update_distribution_status(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _status: &str,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }
    async fn update_redaction_status(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _status: &str,
        _error: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }
    async fn complete_redaction(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _redacted_r2_key: &str,
        _redactions_applied: i32,
        _timing: &alc_core::repository::notify_documents::RedactTiming,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }
    async fn reset_redaction(&self, _tenant_id: Uuid, _id: Uuid) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }
    async fn reset_extraction(&self, _tenant_id: Uuid, _id: Uuid) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }
}

// ============================================================
// MockNotifyDeliveryRepository
// ============================================================

pub struct MockNotifyDeliveryRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockNotifyDeliveryRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

fn mock_delivery(tenant_id: Uuid, document_id: Uuid, recipient_id: Uuid) -> NotifyDelivery {
    NotifyDelivery {
        id: Uuid::new_v4(),
        tenant_id,
        document_id,
        recipient_id,
        provider: "line".into(),
        status: "pending".into(),
        error_message: None,
        attempt: 0,
        sent_at: None,
        read_at: None,
        read_token: Uuid::new_v4(),
        created_at: chrono::Utc::now(),
    }
}

#[async_trait::async_trait]
impl NotifyDeliveryRepository for MockNotifyDeliveryRepository {
    async fn create_batch(
        &self,
        tenant_id: Uuid,
        document_id: Uuid,
        recipients: &[(Uuid, String)],
    ) -> Result<Vec<NotifyDelivery>, sqlx::Error> {
        check_fail!(self);
        Ok(recipients
            .iter()
            .map(|(rid, _)| mock_delivery(tenant_id, document_id, *rid))
            .collect())
    }
    async fn update_status(
        &self,
        _tenant_id: Uuid,
        _id: Uuid,
        _status: &str,
        _error: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }
    async fn mark_sent(&self, _tenant_id: Uuid, _id: Uuid) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }
    async fn list_by_document(
        &self,
        _tenant_id: Uuid,
        document_id: Uuid,
    ) -> Result<Vec<NotifyDeliveryWithRecipient>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![NotifyDeliveryWithRecipient {
            id: Uuid::new_v4(),
            document_id,
            recipient_id: Uuid::new_v4(),
            provider: "line".into(),
            status: "sent".into(),
            error_message: None,
            attempt: 1,
            sent_at: Some(chrono::Utc::now()),
            read_at: None,
            read_token: Uuid::new_v4(),
            created_at: chrono::Utc::now(),
            recipient_name: "Test".into(),
            triggered_by_user_id: None,
            triggered_by_name: None,
            triggered_by_email: None,
        }])
    }
    async fn list_pending(
        &self,
        tenant_id: Uuid,
        document_id: Uuid,
    ) -> Result<Vec<NotifyDelivery>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![mock_delivery(tenant_id, document_id, Uuid::new_v4())])
    }
}

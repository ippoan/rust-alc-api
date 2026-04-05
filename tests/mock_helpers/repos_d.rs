use std::sync::atomic::{AtomicBool, Ordering};

use uuid::Uuid;

use rust_alc_api::db::repository::notify_deliveries::*;
use rust_alc_api::db::repository::notify_documents::*;
use rust_alc_api::db::repository::notify_line_config::*;
use rust_alc_api::db::repository::notify_recipients::*;

macro_rules! check_fail {
    ($self:expr) => {
        if $self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
    };
}

// ============================================================
// MockNotifyRecipientRepository
// ============================================================

pub struct MockNotifyRecipientRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockNotifyRecipientRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

fn mock_recipient(tenant_id: Uuid) -> NotifyRecipient {
    NotifyRecipient {
        id: Uuid::new_v4(),
        tenant_id,
        name: "Test Recipient".into(),
        provider: "line".into(),
        lineworks_user_id: None,
        line_user_id: Some("U1234567890".into()),
        phone_number: None,
        email: None,
        enabled: true,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[async_trait::async_trait]
impl NotifyRecipientRepository for MockNotifyRecipientRepository {
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<NotifyRecipient>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![mock_recipient(tenant_id)])
    }
    async fn get(
        &self,
        tenant_id: Uuid,
        _id: Uuid,
    ) -> Result<Option<NotifyRecipient>, sqlx::Error> {
        check_fail!(self);
        Ok(Some(mock_recipient(tenant_id)))
    }
    async fn create(
        &self,
        tenant_id: Uuid,
        _input: &CreateNotifyRecipient,
    ) -> Result<NotifyRecipient, sqlx::Error> {
        check_fail!(self);
        Ok(mock_recipient(tenant_id))
    }
    async fn update(
        &self,
        tenant_id: Uuid,
        _id: Uuid,
        _input: &UpdateNotifyRecipient,
    ) -> Result<NotifyRecipient, sqlx::Error> {
        check_fail!(self);
        Ok(mock_recipient(tenant_id))
    }
    async fn delete(&self, _tenant_id: Uuid, _id: Uuid) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }
    async fn list_enabled(&self, tenant_id: Uuid) -> Result<Vec<NotifyRecipient>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![mock_recipient(tenant_id)])
    }
    async fn upsert_by_line_user_id(
        &self,
        tenant_id: Uuid,
        _line_user_id: &str,
        _name: &str,
    ) -> Result<NotifyRecipient, sqlx::Error> {
        check_fail!(self);
        Ok(mock_recipient(tenant_id))
    }
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
    async fn mark_read(&self, _read_token: Uuid) -> Result<Option<ReadResult>, sqlx::Error> {
        check_fail!(self);
        Ok(Some(ReadResult {
            document_id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
        }))
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

// ============================================================
// MockNotifyLineConfigRepository
// ============================================================

pub struct MockNotifyLineConfigRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockNotifyLineConfigRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl NotifyLineConfigRepository for MockNotifyLineConfigRepository {
    async fn get(&self, _tenant_id: Uuid) -> Result<Option<NotifyLineConfig>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }
    async fn get_full(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Option<NotifyLineConfigFull>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }
    async fn upsert(
        &self,
        tenant_id: Uuid,
        name: &str,
        channel_id: &str,
        _secret: &str,
        _key_id: &str,
        _private_key: &str,
        _bot_basic_id: Option<&str>,
        _public_key_jwk: Option<&str>,
        _login_channel_id: Option<&str>,
        _login_key_id: Option<&str>,
    ) -> Result<NotifyLineConfig, sqlx::Error> {
        check_fail!(self);
        Ok(NotifyLineConfig {
            id: Uuid::new_v4(),
            tenant_id,
            name: name.into(),
            channel_id: channel_id.into(),
            bot_basic_id: None,
            public_key_jwk: None,
            login_channel_id: None,
            login_key_id: None,
            enabled: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
    }
    async fn delete(&self, _tenant_id: Uuid) -> Result<(), sqlx::Error> {
        check_fail!(self);
        Ok(())
    }
    async fn lookup_by_channel(
        &self,
        _channel_id: &str,
    ) -> Result<Option<NotifyLineConfigFull>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }
}

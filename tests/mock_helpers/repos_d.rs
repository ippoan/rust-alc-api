use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use uuid::Uuid;

use rust_alc_api::db::repository::lineworks_channels::*;
use rust_alc_api::db::repository::notify_deliveries::*;
use rust_alc_api::db::repository::notify_documents::*;
use rust_alc_api::db::repository::notify_recipients::*;

macro_rules! check_fail {
    ($self:expr) => {
        if $self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
    };
}

// ============================================================
// MockLineworksChannelsRepository
// ============================================================

pub struct MockLineworksChannelsRepository {
    pub fail_next: AtomicBool,
    /// `lookup_bot_config_for_webhook` の戻り値を注入できる。
    /// `None` なら従来通り bot_not_found 相当 (`Ok(None)`) を返す。
    pub bot_config: Mutex<Option<BotConfigForWebhook>>,
    /// `upsert_joined` の呼び出し回数 (テスト assertion 用)
    pub upsert_joined_calls: std::sync::atomic::AtomicUsize,
    /// `mark_left` の呼び出し回数 (テスト assertion 用)
    pub mark_left_calls: std::sync::atomic::AtomicUsize,
}

impl Default for MockLineworksChannelsRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            bot_config: Mutex::new(None),
            upsert_joined_calls: std::sync::atomic::AtomicUsize::new(0),
            mark_left_calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

fn mock_lineworks_channel(tenant_id: Uuid) -> LineworksChannel {
    LineworksChannel {
        id: Uuid::new_v4(),
        tenant_id,
        bot_config_id: Uuid::new_v4(),
        channel_id: "ch_mock".into(),
        title: Some("Mock Group".into()),
        channel_type: Some("group".into()),
        joined_at: chrono::Utc::now(),
        active: true,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[async_trait::async_trait]
impl LineworksChannelsRepository for MockLineworksChannelsRepository {
    async fn upsert_joined(
        &self,
        tenant_id: Uuid,
        _bot_config_id: Uuid,
        _channel_id: &str,
        _channel_type: Option<&str>,
        _title: Option<&str>,
    ) -> Result<LineworksChannel, sqlx::Error> {
        check_fail!(self);
        self.upsert_joined_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(mock_lineworks_channel(tenant_id))
    }
    async fn mark_left(
        &self,
        _tenant_id: Uuid,
        _bot_config_id: Uuid,
        _channel_id: &str,
    ) -> Result<(), sqlx::Error> {
        check_fail!(self);
        self.mark_left_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
    async fn lookup_bot_config_for_webhook(
        &self,
        _bot_id: &str,
    ) -> Result<Option<BotConfigForWebhook>, sqlx::Error> {
        check_fail!(self);
        Ok(self.bot_config.lock().unwrap().clone())
    }
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

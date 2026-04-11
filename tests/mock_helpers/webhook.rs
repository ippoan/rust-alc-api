use std::sync::atomic::{AtomicUsize, Ordering};

/// Mock WebhookService for testing webhook fire_event paths.
pub struct MockWebhookService {
    pub fired: AtomicUsize,
}

impl Default for MockWebhookService {
    fn default() -> Self {
        Self {
            fired: AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl rust_alc_api::webhook::WebhookService for MockWebhookService {
    async fn fire_event(
        &self,
        _tenant_id: uuid::Uuid,
        _event_type: &str,
        _payload: serde_json::Value,
    ) {
        self.fired.fetch_add(1, Ordering::SeqCst);
    }
}

/// Mock TroubleNotifier for testing LINE WORKS Bot notification paths.
pub struct MockTroubleNotifier {
    pub notified: AtomicUsize,
}

impl Default for MockTroubleNotifier {
    fn default() -> Self {
        Self {
            notified: AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl alc_trouble::notifier::TroubleNotifier for MockTroubleNotifier {
    async fn notify(
        &self,
        _tenant_id: uuid::Uuid,
        _event_type: &str,
        _message: &str,
        _lineworks_user_ids: &[String],
    ) {
        self.notified.fetch_add(1, Ordering::SeqCst);
    }
}

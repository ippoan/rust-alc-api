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

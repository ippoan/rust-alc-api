use std::sync::atomic::{AtomicBool, Ordering};

use chrono::Utc;
use uuid::Uuid;

use rust_alc_api::db::models::{
    CreateTroubleComment, CreateTroubleTicket, CreateWorkflowState, CreateWorkflowTransition,
    TroubleComment, TroubleFile, TroubleStatusHistory, TroubleTicket, TroubleTicketFilter,
    TroubleTicketsResponse, TroubleWorkflowState, TroubleWorkflowTransition, UpdateTroubleTicket,
};
use rust_alc_api::db::repository::{
    TroubleCommentsRepository, TroubleFilesRepository, TroubleTicketsRepository,
    TroubleWorkflowRepository,
};

macro_rules! check_fail {
    ($self:expr) => {
        if $self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(sqlx::Error::RowNotFound);
        }
    };
}

// ============================================================
// MockTroubleTicketsRepository
// ============================================================

pub struct MockTroubleTicketsRepository {
    pub fail_next: AtomicBool,
    pub return_some: AtomicBool,
}

impl Default for MockTroubleTicketsRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            return_some: AtomicBool::new(false),
        }
    }
}

fn mock_ticket(tenant_id: Uuid) -> TroubleTicket {
    TroubleTicket {
        id: Uuid::new_v4(),
        tenant_id,
        ticket_no: 1,
        category: "貨物事故".to_string(),
        title: "test".to_string(),
        occurred_at: None,
        occurred_date: None,
        company_name: "テスト会社".to_string(),
        office_name: "本社".to_string(),
        department: "第1運行課".to_string(),
        person_name: "テスト太郎".to_string(),
        person_id: None,
        vehicle_number: "".to_string(),
        location: "".to_string(),
        description: "test description".to_string(),
        status_id: None,
        assigned_to: None,
        progress_notes: "".to_string(),
        allowance: "".to_string(),
        damage_amount: None,
        compensation_amount: None,
        confirmation_notice: "".to_string(),
        disciplinary_content: "".to_string(),
        road_service_cost: None,
        counterparty: "".to_string(),
        counterparty_insurance: "".to_string(),
        custom_fields: serde_json::json!({}),
        due_date: None,
        overdue_notified_at: None,
        created_by: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        deleted_at: None,
    }
}

#[async_trait::async_trait]
impl TroubleTicketsRepository for MockTroubleTicketsRepository {
    async fn create(
        &self,
        tenant_id: Uuid,
        input: &CreateTroubleTicket,
        _created_by: Option<Uuid>,
        _initial_status_id: Option<Uuid>,
    ) -> Result<TroubleTicket, sqlx::Error> {
        check_fail!(self);
        let mut ticket = mock_ticket(tenant_id);
        ticket.category = input.category.clone();
        ticket.title = input.title.clone().unwrap_or_default();
        ticket.description = input.description.clone().unwrap_or_default();
        Ok(ticket)
    }

    async fn list(
        &self,
        _tenant_id: Uuid,
        filter: &TroubleTicketFilter,
    ) -> Result<TroubleTicketsResponse, sqlx::Error> {
        check_fail!(self);
        Ok(TroubleTicketsResponse {
            tickets: vec![],
            total: 0,
            page: filter.page.unwrap_or(1),
            per_page: filter.per_page.unwrap_or(50),
        })
    }

    async fn get(&self, tenant_id: Uuid, _id: Uuid) -> Result<Option<TroubleTicket>, sqlx::Error> {
        check_fail!(self);
        if self.return_some.load(Ordering::SeqCst) {
            Ok(Some(mock_ticket(tenant_id)))
        } else {
            Ok(None)
        }
    }

    async fn update(
        &self,
        tenant_id: Uuid,
        _id: Uuid,
        _input: &UpdateTroubleTicket,
    ) -> Result<Option<TroubleTicket>, sqlx::Error> {
        check_fail!(self);
        if self.return_some.load(Ordering::SeqCst) {
            Ok(Some(mock_ticket(tenant_id)))
        } else {
            Ok(None)
        }
    }

    async fn soft_delete(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(true)
    }

    async fn update_status(
        &self,
        tenant_id: Uuid,
        _id: Uuid,
        _status_id: Uuid,
    ) -> Result<Option<TroubleTicket>, sqlx::Error> {
        check_fail!(self);
        if self.return_some.load(Ordering::SeqCst) {
            Ok(Some(mock_ticket(tenant_id)))
        } else {
            Ok(None)
        }
    }
}

// ============================================================
// MockTroubleFilesRepository
// ============================================================

pub struct MockTroubleFilesRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockTroubleFilesRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl TroubleFilesRepository for MockTroubleFilesRepository {
    async fn create(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
        filename: &str,
        content_type: &str,
        size_bytes: i64,
        storage_key: &str,
    ) -> Result<TroubleFile, sqlx::Error> {
        check_fail!(self);
        Ok(TroubleFile {
            id: Uuid::new_v4(),
            tenant_id,
            ticket_id,
            filename: filename.to_string(),
            content_type: content_type.to_string(),
            size_bytes,
            storage_key: storage_key.to_string(),
            created_at: Utc::now(),
        })
    }

    async fn list_by_ticket(
        &self,
        _tenant_id: Uuid,
        _ticket_id: Uuid,
    ) -> Result<Vec<TroubleFile>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn get(&self, _tenant_id: Uuid, _id: Uuid) -> Result<Option<TroubleFile>, sqlx::Error> {
        check_fail!(self);
        Ok(None)
    }

    async fn delete(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(true)
    }
}

// ============================================================
// MockTroubleWorkflowRepository
// ============================================================

pub struct MockTroubleWorkflowRepository {
    pub fail_next: AtomicBool,
    pub return_initial: AtomicBool,
}

impl Default for MockTroubleWorkflowRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
            return_initial: AtomicBool::new(false),
        }
    }
}

fn mock_workflow_state(tenant_id: Uuid) -> TroubleWorkflowState {
    TroubleWorkflowState {
        id: Uuid::new_v4(),
        tenant_id,
        name: "new".to_string(),
        label: "新規".to_string(),
        color: "#3B82F6".to_string(),
        sort_order: 1,
        is_initial: true,
        is_terminal: false,
        created_at: Utc::now(),
    }
}

#[async_trait::async_trait]
impl TroubleWorkflowRepository for MockTroubleWorkflowRepository {
    async fn list_states(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<TroubleWorkflowState>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn create_state(
        &self,
        tenant_id: Uuid,
        input: &CreateWorkflowState,
    ) -> Result<TroubleWorkflowState, sqlx::Error> {
        check_fail!(self);
        let mut state = mock_workflow_state(tenant_id);
        state.name = input.name.clone();
        state.label = input.label.clone();
        if let Some(ref color) = input.color {
            state.color = color.clone();
        }
        if let Some(order) = input.sort_order {
            state.sort_order = order;
        }
        if let Some(initial) = input.is_initial {
            state.is_initial = initial;
        }
        if let Some(terminal) = input.is_terminal {
            state.is_terminal = terminal;
        }
        Ok(state)
    }

    async fn delete_state(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(true)
    }

    async fn list_transitions(
        &self,
        _tenant_id: Uuid,
    ) -> Result<Vec<TroubleWorkflowTransition>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn create_transition(
        &self,
        tenant_id: Uuid,
        input: &CreateWorkflowTransition,
    ) -> Result<TroubleWorkflowTransition, sqlx::Error> {
        check_fail!(self);
        Ok(TroubleWorkflowTransition {
            id: Uuid::new_v4(),
            tenant_id,
            from_state_id: input.from_state_id,
            to_state_id: input.to_state_id,
            label: input.label.clone(),
            created_at: Utc::now(),
        })
    }

    async fn delete_transition(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(true)
    }

    async fn is_transition_allowed(
        &self,
        _tenant_id: Uuid,
        _from_state_id: Option<Uuid>,
        _to_state_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(true)
    }

    async fn get_initial_state(
        &self,
        tenant_id: Uuid,
    ) -> Result<Option<TroubleWorkflowState>, sqlx::Error> {
        check_fail!(self);
        if self.return_initial.load(Ordering::SeqCst) {
            Ok(Some(mock_workflow_state(tenant_id)))
        } else {
            Ok(None)
        }
    }

    async fn record_history(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
        from_state_id: Option<Uuid>,
        to_state_id: Uuid,
        changed_by: Option<Uuid>,
        comment: Option<String>,
    ) -> Result<TroubleStatusHistory, sqlx::Error> {
        check_fail!(self);
        Ok(TroubleStatusHistory {
            id: Uuid::new_v4(),
            tenant_id,
            ticket_id,
            from_state_id,
            to_state_id,
            changed_by,
            comment: comment.unwrap_or_default(),
            created_at: Utc::now(),
        })
    }

    async fn list_history(
        &self,
        _tenant_id: Uuid,
        _ticket_id: Uuid,
    ) -> Result<Vec<TroubleStatusHistory>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn setup_defaults(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<TroubleWorkflowState>, sqlx::Error> {
        check_fail!(self);
        let defaults = [
            ("new", "新規", "#3B82F6", 1, true, false),
            ("in_progress", "対応中", "#F59E0B", 2, false, false),
            ("resolved", "解決", "#10B981", 3, false, false),
            ("closed", "完了", "#6B7280", 4, false, true),
        ];
        Ok(defaults
            .iter()
            .map(
                |(name, label, color, order, initial, terminal)| TroubleWorkflowState {
                    id: Uuid::new_v4(),
                    tenant_id,
                    name: name.to_string(),
                    label: label.to_string(),
                    color: color.to_string(),
                    sort_order: *order,
                    is_initial: *initial,
                    is_terminal: *terminal,
                    created_at: Utc::now(),
                },
            )
            .collect())
    }
}

// ============================================================
// MockTroubleCommentsRepository
// ============================================================

pub struct MockTroubleCommentsRepository {
    pub fail_next: AtomicBool,
}

impl Default for MockTroubleCommentsRepository {
    fn default() -> Self {
        Self {
            fail_next: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl TroubleCommentsRepository for MockTroubleCommentsRepository {
    async fn create(
        &self,
        tenant_id: Uuid,
        ticket_id: Uuid,
        author_id: Option<Uuid>,
        input: &CreateTroubleComment,
    ) -> Result<TroubleComment, sqlx::Error> {
        check_fail!(self);
        Ok(TroubleComment {
            id: Uuid::new_v4(),
            tenant_id,
            ticket_id,
            author_id,
            body: input.body.clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    async fn list_by_ticket(
        &self,
        _tenant_id: Uuid,
        _ticket_id: Uuid,
    ) -> Result<Vec<TroubleComment>, sqlx::Error> {
        check_fail!(self);
        Ok(vec![])
    }

    async fn delete(&self, _tenant_id: Uuid, _id: Uuid) -> Result<bool, sqlx::Error> {
        check_fail!(self);
        Ok(true)
    }
}

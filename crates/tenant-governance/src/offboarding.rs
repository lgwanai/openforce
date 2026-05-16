use uuid::Uuid;

/// Tenant offboarding states (architecture doc section 28.4)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OffboardingState {
    Active,
    Frozen,
    Exporting,
    Deleting,
    CleaningBackups,
    Offboarded,
}

pub struct OffboardingFlow {
    pub tenant_id: Uuid,
    pub state: OffboardingState,
    steps: Vec<OffboardingStep>,
}

#[derive(Debug, Clone)]
struct OffboardingStep {
    name: String,
    completed: bool,
}

impl OffboardingFlow {
    pub fn new(tenant_id: Uuid) -> Self {
        let steps = vec![
            "freeze_new_tasks", "export_data", "stop_observer_sampling",
            "delete_online_objects", "clean_staging_outbox",
            "clean_backups", "destroy_tenant_key",
        ];
        Self {
            tenant_id,
            state: OffboardingState::Active,
            steps: steps.into_iter().map(|s| OffboardingStep { name: s.into(), completed: false }).collect(),
        }
    }

    pub fn start(&mut self) { self.state = OffboardingState::Frozen; }

    pub fn complete_step(&mut self, step_name: &str) {
        if let Some(s) = self.steps.iter_mut().find(|s| s.name == step_name) {
            s.completed = true;
        }
        if self.steps.iter().all(|s| s.completed) {
            self.state = OffboardingState::Offboarded;
        }
    }
}

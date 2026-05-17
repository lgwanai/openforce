use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Producer identity carried on every event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProducerIdentity {
    pub component: String,
    pub instance_id: String,
    pub region: String,
}

/// Unified event envelope (architecture doc section 16)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub event_id: Uuid,
    pub event_type: String,
    pub session_id: Uuid,
    pub tenant_id: Uuid,
    pub plan_version: i64,
    pub plan_epoch: i32,
    pub task_id: Option<Uuid>,
    pub task_attempt: i32,
    pub producer: ProducerIdentity,
    pub causation_id: Uuid,
    pub correlation_id: Uuid,
    pub occurred_at: DateTime<Utc>,
    pub session_version: i64,
    pub payload: EventPayload,
}

impl EventEnvelope {
    pub fn new(
        event_type: &str,
        session_id: Uuid,
        tenant_id: Uuid,
        producer: ProducerIdentity,
        payload: EventPayload,
    ) -> Self {
        let causation_id = Uuid::now_v7();
        Self {
            event_id: Uuid::now_v7(),
            event_type: event_type.to_string(),
            session_id,
            tenant_id,
            plan_version: 0,
            plan_epoch: 0,
            task_id: None,
            task_attempt: 0,
            producer,
            causation_id,
            correlation_id: causation_id,
            occurred_at: Utc::now(),
            session_version: 0,
            payload,
        }
    }

    /// Set explicit causation_id for audit traceability (Section 16.2).
    /// Callers SHOULD use this instead of relying on auto-generated IDs.
    pub fn with_causation(mut self, causation_id: Uuid, correlation_id: Uuid) -> Self {
        self.causation_id = causation_id;
        self.correlation_id = correlation_id;
        self
    }

    pub fn with_task(
        mut self,
        task_id: Uuid,
        task_attempt: i32,
        plan_version: i64,
        plan_epoch: i32,
    ) -> Self {
        self.task_id = Some(task_id);
        self.task_attempt = task_attempt;
        self.plan_version = plan_version;
        self.plan_epoch = plan_epoch;
        self
    }
}

/// All event-specific payloads (architecture doc sections 3, 16, 23)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventPayload {
    SessionCreated(SessionCreatedPayload),
    PlanProposed(PlanProposedPayload),
    PlanCompiled(PlanCompiledPayload),
    PlanEpochStarted(PlanEpochStartedPayload),
    EpochClosed(EpochClosedPayload),
    TaskReadied(TaskReadiedPayload),
    TaskLeased(TaskLeasedPayload),
    TaskStarted(TaskStartedPayload),
    HeartbeatReceived(HeartbeatReceivedPayload),
    ArtifactSubmitted(ArtifactSubmittedPayload),
    PatchSubmitted(PatchSubmittedPayload),
    FindingSubmitted(FindingSubmittedPayload),
    EffectRequested(EffectRequestedPayload),
    EffectApproved(EffectApprovedPayload),
    EffectRejected(EffectRejectedPayload),
    EffectCommitted(EffectCommittedPayload),
    TaskSucceeded(TaskSucceededPayload),
    TaskFailed(TaskFailedPayload),
    TaskTimedOut(TaskTimedOutPayload),
    TaskCancelled(TaskCancelledPayload),
    TaskReassigned(TaskReassignedPayload),
    TaskInheritedToEpoch(TaskInheritedToEpochPayload),
    TaskFrozenByReplan(TaskFrozenByReplanPayload),
    TaskInvalidatedByReplan(TaskInvalidatedByReplanPayload),
    SessionCompleted(SessionCompletedPayload),
    SessionAborted(SessionAbortedPayload),
    PhaseAdvanced(PhaseAdvancedPayload),
    GateCreated(GateCreatedPayload),
    GateResolved(GateResolvedPayload),
    UserFeedbackReceived(UserFeedbackPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseAdvancedPayload {
    pub from_phase: String,
    pub to_phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateCreatedPayload {
    pub gate_id: String,
    pub phase: String,
    pub artifact_summary: Option<String>,
    pub plan_epoch: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateResolvedPayload {
    pub gate_id: String,
    pub resolution: String,
    pub user_feedback: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserFeedbackPayload {
    pub feedback: String,
    pub plan_epoch: i32,
}

// --- Payload structs ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCreatedPayload {
    pub goal: String,
    pub policy_profile: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanProposedPayload {
    pub candidate_plans: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanCompiledPayload {
    pub plan_version: i32,
    pub plan_epoch: i32,
    pub tasks: Vec<TaskDef>,
    pub dag_edges: Vec<DagEdgeDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDef {
    pub task_id: String,
    pub task_type: String,
    pub priority: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagEdgeDef {
    pub from_task_id: String,
    pub to_task_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanEpochStartedPayload {
    pub plan_epoch: i32,
    pub plan_version: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpochClosedPayload {
    pub plan_epoch: i32,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskReadiedPayload {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskLeasedPayload {
    pub lease_id: String,
    pub lease_expire_at: String,
    pub renewal_deadline: String,
    pub fencing_token: u64,
    pub worker_spec_id: String,
    pub assigned_node_pool: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStartedPayload {
    pub lease_id: String,
    pub node_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatReceivedPayload {
    pub lease_id: String,
    pub fencing_token: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactSubmittedPayload {
    pub artifact_id: String,
    pub artifact_type: String,
    pub storage_uri: String,
    pub content_sha256: String,
    pub produced_by_spec: String,
    pub base_snapshot_id: String,
    pub validation_summary: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchSubmittedPayload {
    pub patch_ref: String,
    pub patch_sha256: String,
    pub target_paths: Vec<String>,
    pub base_snapshot_id: String,
    pub merge_commit_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingSubmittedPayload {
    pub finding_type: String,
    pub finding_data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectRequestedPayload {
    pub effect_id: String,
    pub effect_type: String,
    pub idempotency_key: String,
    pub requested_by_task: String,
    pub request_ref: String,
    pub approval_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectApprovedPayload {
    pub effect_id: String,
    pub approved_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectRejectedPayload {
    pub effect_id: String,
    pub rejected_by: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectCommittedPayload {
    pub effect_id: String,
    pub execution_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSucceededPayload {
    pub lease_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskFailedPayload {
    pub reason: String,
    pub recoverable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTimedOutPayload {
    pub lease_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCancelledPayload {
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskReassignedPayload {
    pub new_task_id: String,
    pub new_lease_id: String,
    pub new_fencing_token: u64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInheritedToEpochPayload {
    pub from_plan_epoch: i32,
    pub to_plan_epoch: i32,
    pub new_task_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskFrozenByReplanPayload {
    pub from_plan_epoch: i32,
    pub to_plan_epoch: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInvalidatedByReplanPayload {
    pub from_plan_epoch: i32,
    pub to_plan_epoch: i32,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCompletedPayload {
    pub summary: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionAbortedPayload {
    pub reason: String,
}

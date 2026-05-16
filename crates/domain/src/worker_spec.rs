use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkerSpecId(pub Uuid);

impl WorkerSpecId {
    pub fn new() -> Self { Self(Uuid::now_v7()) }
}

/// A frozen Worker Spec — the immutable execution contract for a single task attempt.
/// Architecture doc section 17 defines all fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSpec {
    pub worker_spec_id: Uuid,
    pub session_id: Uuid,
    pub plan_version: i32,
    pub plan_epoch: i32,
    pub task: WorkerSpecTask,
    pub lease: WorkerSpecLease,
    pub runtime: WorkerSpecRuntime,
    pub model: WorkerSpecModel,
    pub prompt_bundle: WorkerSpecPromptBundle,
    pub inputs: WorkerSpecInputs,
    pub tool_policy: WorkerSpecToolPolicy,
    pub network_policy: WorkerSpecNetworkPolicy,
    pub effect_policy: WorkerSpecEffectPolicy,
    pub budget: WorkerSpecBudget,
    pub acceptance_contract: WorkerSpecAcceptanceContract,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSpecTask {
    pub task_id: Uuid,
    pub task_type: String,
    pub task_attempt: i32,
    pub priority: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSpecLease {
    pub lease_id: Uuid,
    pub fencing_token: u64,
    pub lease_expire_at: String,
    pub heartbeat_interval_sec: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSpecRuntime {
    pub sandbox_image: String,
    pub sandbox_class: String,
    pub cpu_limit: i32,
    pub memory_mb: i32,
    pub workspace_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSpecModel {
    pub provider: String,
    pub model_name: String,
    pub temperature: f64,
    pub max_tokens: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSpecPromptBundle {
    pub role_profile_version: String,
    pub prompt_bundle_version: String,
    pub local_sop_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSpecInputs {
    pub snapshot_id: String,
    pub artifacts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSpecToolPolicy {
    pub tool_policy_version: String,
    pub allowed_read_paths: Vec<String>,
    pub allowed_write_paths: Vec<String>,
    pub forbidden_paths: Vec<String>,
    pub allowed_tools: serde_json::Value,
    pub blocked_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSpecNetworkPolicy {
    pub mode: String,
    pub allowlist: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSpecEffectPolicy {
    pub allowed_effects: Vec<String>,
    pub approval_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSpecBudget {
    pub max_wall_clock_sec: i32,
    pub max_tool_calls: i32,
    pub max_patch_attempts: i32,
    pub max_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSpecAcceptanceContract {
    pub required_checks: Vec<String>,
    pub required_outputs: Vec<String>,
}

/// Builder pattern for constructing a frozen WorkerSpec
pub struct WorkerSpecBuilder {
    spec: WorkerSpec,
}

impl WorkerSpecBuilder {
    pub fn new(
        session_id: Uuid,
        task_id: Uuid,
        task_type: String,
        task_attempt: i32,
        plan_version: i32,
        plan_epoch: i32,
    ) -> Self {
        let now = chrono::Utc::now();
        let expire_at = now + chrono::Duration::seconds(300);

        Self {
            spec: WorkerSpec {
                worker_spec_id: Uuid::now_v7(),
                session_id,
                plan_version,
                plan_epoch,
                task: WorkerSpecTask {
                    task_id,
                    task_type,
                    task_attempt,
                    priority: "normal".into(),
                },
                lease: WorkerSpecLease {
                    lease_id: Uuid::nil(),
                    fencing_token: 0,
                    lease_expire_at: expire_at.to_rfc3339(),
                    heartbeat_interval_sec: 15,
                },
                runtime: WorkerSpecRuntime {
                    sandbox_image: String::new(),
                    sandbox_class: "firecracker-medium".into(),
                    cpu_limit: 2,
                    memory_mb: 4096,
                    workspace_mode: "ephemeral".into(),
                },
                model: WorkerSpecModel {
                    provider: "openai".into(),
                    model_name: "gpt-4.1".into(),
                    temperature: 0.2,
                    max_tokens: 16000,
                },
                prompt_bundle: WorkerSpecPromptBundle {
                    role_profile_version: String::new(),
                    prompt_bundle_version: String::new(),
                    local_sop_version: String::new(),
                },
                inputs: WorkerSpecInputs {
                    snapshot_id: String::new(),
                    artifacts: vec![],
                },
                tool_policy: WorkerSpecToolPolicy {
                    tool_policy_version: String::new(),
                    allowed_read_paths: vec![],
                    allowed_write_paths: vec![],
                    forbidden_paths: vec![],
                    allowed_tools: serde_json::json!({}),
                    blocked_tools: vec![],
                },
                network_policy: WorkerSpecNetworkPolicy {
                    mode: "deny_by_default".into(),
                    allowlist: vec![],
                },
                effect_policy: WorkerSpecEffectPolicy {
                    allowed_effects: vec![],
                    approval_required: true,
                },
                budget: WorkerSpecBudget {
                    max_wall_clock_sec: 900,
                    max_tool_calls: 40,
                    max_patch_attempts: 6,
                    max_cost_usd: 1.8,
                },
                acceptance_contract: WorkerSpecAcceptanceContract {
                    required_checks: vec![],
                    required_outputs: vec![],
                },
            },
        }
    }

    pub fn with_lease(mut self, lease_id: Uuid, fencing_token: u64, expire_at: String) -> Self {
        self.spec.lease.lease_id = lease_id;
        self.spec.lease.fencing_token = fencing_token;
        self.spec.lease.lease_expire_at = expire_at;
        self
    }

    pub fn with_model(mut self, provider: String, model_name: String) -> Self {
        self.spec.model.provider = provider;
        self.spec.model.model_name = model_name;
        self
    }

    pub fn build(self) -> WorkerSpec {
        self.spec
    }
}

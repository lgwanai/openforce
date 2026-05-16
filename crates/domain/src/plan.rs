use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Plan version is an integer that increments with each compilation
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PlanVersion(pub i32);

impl Default for PlanVersion {
    fn default() -> Self { Self(0) }
}

impl PlanVersion {
    pub fn next(self) -> Self { Self(self.0 + 1) }
    pub fn value(&self) -> i32 { self.0 }
}

/// PlanEpoch is the logical boundary of a compiled plan (architecture doc section 23)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanEpoch(pub i32);

impl Default for PlanEpoch {
    fn default() -> Self { Self(0) }
}

impl PlanEpoch {
    pub fn next(self) -> Self { Self(self.0 + 1) }
    pub fn value(&self) -> i32 { self.0 }
}

/// A node in the execution DAG
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNode {
    pub task_id: Uuid,
    pub task_type: String,
    pub priority: String,
    pub upstream_ids: Vec<Uuid>,
    pub downstream_ids: Vec<Uuid>,
}

/// A compiled plan with its DAG
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub plan_version: PlanVersion,
    pub plan_epoch: PlanEpoch,
    pub session_id: Uuid,
    pub nodes: Vec<DagNode>,
}

impl Plan {
    pub fn new(
        plan_version: PlanVersion,
        plan_epoch: PlanEpoch,
        session_id: Uuid,
    ) -> Self {
        Self {
            plan_version,
            plan_epoch,
            session_id,
            nodes: vec![],
        }
    }

    pub fn add_node(&mut self, node: DagNode) {
        self.nodes.push(node);
    }

    pub fn find_node(&self, task_id: Uuid) -> Option<&DagNode> {
        self.nodes.iter().find(|n| n.task_id == task_id)
    }
}

/// Classification for old-plan tasks when a new plan epoch is compiled (section 23.3)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EpochMappingMode {
    /// Semantically unchanged — continues under new epoch
    Inherited,
    /// Temporarily frozen — waits for scheduler decision
    Frozen,
    /// Semantically invalid — must be rejected immediately
    Invalidated,
}

impl EpochMappingMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Inherited => "inherited",
            Self::Frozen => "frozen",
            Self::Invalidated => "invalidated",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "inherited" => Some(Self::Inherited),
            "frozen" => Some(Self::Frozen),
            "invalidated" => Some(Self::Invalidated),
            _ => None,
        }
    }
}

/// Maps an old-plan task to a new-plan task (section 23.6)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpochMapping {
    pub from_plan_epoch: i32,
    pub to_plan_epoch: i32,
    pub from_task_id: Uuid,
    pub to_task_id: Option<Uuid>,
    pub mode: EpochMappingMode,
    pub reason: Option<String>,
}

impl EpochMapping {
    pub fn new(
        from_plan_epoch: i32,
        to_plan_epoch: i32,
        from_task_id: Uuid,
        mode: EpochMappingMode,
    ) -> Self {
        Self {
            from_plan_epoch,
            to_plan_epoch,
            from_task_id,
            to_task_id: None,
            mode,
            reason: None,
        }
    }

    pub fn with_new_task(mut self, new_task_id: Uuid) -> Self {
        self.to_task_id = Some(new_task_id);
        self
    }

    pub fn with_reason(mut self, reason: String) -> Self {
        self.reason = Some(reason);
        self
    }
}

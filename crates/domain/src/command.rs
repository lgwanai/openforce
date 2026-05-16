use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::event::ProducerIdentity;
use crate::error::DomainResult;

/// Command types that drive all state transitions (architecture doc section 21)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandType {
    CompilePlan,
    MarkTaskReady,
    LeaseTask,
    RenewLease,
    SubmitArtifact,
    SubmitPatch,
    SubmitFinding,
    RequestEffect,
    MarkTaskSucceeded,
    MarkTaskTimedOut,
    ReplanSession,
    CancelTask,
}

impl CommandType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CompilePlan => "CompilePlan",
            Self::MarkTaskReady => "MarkTaskReady",
            Self::LeaseTask => "LeaseTask",
            Self::RenewLease => "RenewLease",
            Self::SubmitArtifact => "SubmitArtifact",
            Self::SubmitPatch => "SubmitPatch",
            Self::SubmitFinding => "SubmitFinding",
            Self::RequestEffect => "RequestEffect",
            Self::MarkTaskSucceeded => "MarkTaskSucceeded",
            Self::MarkTaskTimedOut => "MarkTaskTimedOut",
            Self::ReplanSession => "ReplanSession",
            Self::CancelTask => "CancelTask",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "CompilePlan" => Some(Self::CompilePlan),
            "MarkTaskReady" => Some(Self::MarkTaskReady),
            "LeaseTask" => Some(Self::LeaseTask),
            "RenewLease" => Some(Self::RenewLease),
            "SubmitArtifact" => Some(Self::SubmitArtifact),
            "SubmitPatch" => Some(Self::SubmitPatch),
            "SubmitFinding" => Some(Self::SubmitFinding),
            "RequestEffect" => Some(Self::RequestEffect),
            "MarkTaskSucceeded" => Some(Self::MarkTaskSucceeded),
            "MarkTaskTimedOut" => Some(Self::MarkTaskTimedOut),
            "ReplanSession" => Some(Self::ReplanSession),
            "CancelTask" => Some(Self::CancelTask),
            _ => None,
        }
    }
}

/// Command envelope with CAS expected_version (architecture doc section 21)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub command_id: Uuid,
    pub command_type: CommandType,
    pub tenant_id: Uuid,
    pub session_id: Uuid,
    pub task_id: Option<Uuid>,
    pub expected_version: i64,
    pub requested_by: ProducerIdentity,
    pub requested_at: DateTime<Utc>,
    pub payload: serde_json::Value,
}

impl Command {
    pub fn new(
        command_type: CommandType,
        tenant_id: Uuid,
        session_id: Uuid,
        expected_version: i64,
        requested_by: ProducerIdentity,
    ) -> Self {
        Self {
            command_id: Uuid::now_v7(),
            command_type,
            tenant_id,
            session_id,
            task_id: None,
            expected_version,
            requested_by,
            requested_at: Utc::now(),
            payload: serde_json::Value::Null,
        }
    }

    pub fn with_task_id(mut self, task_id: Uuid) -> Self {
        self.task_id = Some(task_id);
        self
    }

    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = payload;
        self
    }
}

/// Result of executing a command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    pub command_id: Uuid,
    pub success: bool,
    pub new_session_version: i64,
    pub result: serde_json::Value,
}

/// Trait for command dedup storage
#[async_trait::async_trait]
pub trait CommandDedupStore: Send + Sync {
    async fn check_and_save(
        &self,
        command_id: Uuid,
        command_type: CommandType,
        session_id: Uuid,
    ) -> DomainResult<Option<CommandResult>>;

    async fn save_result(&self, command_id: Uuid, result: &CommandResult) -> DomainResult<()>;
}

/// Trait for command handling dispatch
#[async_trait::async_trait]
pub trait CommandHandler: Send + Sync {
    async fn execute(&self, command: Command) -> DomainResult<CommandResult>;
}

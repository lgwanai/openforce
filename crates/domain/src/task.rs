use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{DomainError, DomainResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub Uuid);

impl TaskId {
    pub fn new() -> Self { Self(Uuid::now_v7()) }
}

impl From<Uuid> for TaskId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskState {
    Pending,
    Ready,
    Leased,
    Running,
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
}

impl TaskState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Ready => "Ready",
            Self::Leased => "Leased",
            Self::Running => "Running",
            Self::Succeeded => "Succeeded",
            Self::Failed => "Failed",
            Self::TimedOut => "TimedOut",
            Self::Cancelled => "Cancelled",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Pending" => Some(Self::Pending),
            "Ready" => Some(Self::Ready),
            "Leased" => Some(Self::Leased),
            "Running" => Some(Self::Running),
            "Succeeded" => Some(Self::Succeeded),
            "Failed" => Some(Self::Failed),
            "TimedOut" => Some(Self::TimedOut),
            "Cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    /// Validate a state transition. Returns Ok(()) if legal, Err otherwise.
    pub fn can_transition_to(self, next: TaskState) -> DomainResult<()> {
        use TaskState::*;
        match (self, next) {
            (Pending, Ready) => Ok(()),
            (Ready, Leased) => Ok(()),
            (Leased, Running) => Ok(()),
            (Leased, TimedOut) => Ok(()),
            (Running, Succeeded) => Ok(()),
            (Running, Failed) => Ok(()),
            (Running, TimedOut) => Ok(()),
            (Running, Cancelled) => Ok(()),
            (TimedOut, Ready) => Ok(()),
            (Failed, Ready) => Ok(()),
            (Ready, Cancelled) => Ok(()),
            (Pending, Cancelled) => Ok(()),
            _ => Err(DomainError::InvalidTransition {
                from: self.as_str().into(),
                to: next.as_str().into(),
            }),
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }

    pub fn is_runnable(&self) -> bool {
        matches!(self, Self::Running | Self::Leased)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplanDisposition {
    Active,
    Inherited,
    Frozen,
    Invalidated,
}

impl ReplanDisposition {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Inherited => "inherited",
            Self::Frozen => "frozen",
            Self::Invalidated => "invalidated",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "active" => Some(Self::Active),
            "inherited" => Some(Self::Inherited),
            "frozen" => Some(Self::Frozen),
            "invalidated" => Some(Self::Invalidated),
            _ => None,
        }
    }

    pub fn can_submit(&self) -> bool {
        matches!(self, Self::Active | Self::Inherited)
    }
}

/// Task aggregate root for projection reads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub session_id: Uuid,
    pub task_id: Uuid,
    pub task_type: String,
    pub state: TaskState,
    pub task_attempt: i32,
    pub current_lease_id: Option<Uuid>,
    pub current_fencing_token: u64,
    pub current_worker_spec_id: Option<Uuid>,
    pub plan_epoch: i32,
    pub replan_disposition: ReplanDisposition,
    pub upstream_task_ids: Vec<Uuid>,
    pub downstream_task_ids: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Task {
    pub fn new(session_id: Uuid, task_id: Uuid, task_type: String) -> Self {
        let now = Utc::now();
        Self {
            session_id,
            task_id,
            task_type,
            state: TaskState::Pending,
            task_attempt: 0,
            current_lease_id: None,
            current_fencing_token: 0,
            current_worker_spec_id: None,
            plan_epoch: 0,
            replan_disposition: ReplanDisposition::Active,
            upstream_task_ids: vec![],
            downstream_task_ids: vec![],
            created_at: now,
            updated_at: now,
        }
    }

    pub fn transition_to(&mut self, new_state: TaskState) -> DomainResult<()> {
        self.state.can_transition_to(new_state)?;
        self.state = new_state;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Verify that a given fencing_token is current for this task
    pub fn verify_fencing(&self, provided: u64) -> DomainResult<()> {
        if provided < self.current_fencing_token {
            return Err(DomainError::FencingTokenStale {
                provided,
                current: self.current_fencing_token,
            });
        }
        Ok(())
    }

    /// Verify the task can accept submissions
    pub fn verify_submissible(&self) -> DomainResult<()> {
        if !self.state.is_runnable() {
            return Err(DomainError::InvalidTransition {
                from: self.state.as_str().into(),
                to: "submit".into(),
            });
        }
        if !self.replan_disposition.can_submit() {
            return Err(DomainError::InvalidTransition {
                from: self.replan_disposition.as_str().into(),
                to: "submit".into(),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum TaskTransitionCondition {
    DependenciesSatisfied,
    LeaseGranted { lease_id: Uuid, fencing_token: u64 },
    HeartbeatReceived,
    ResultSubmitted,
    AcceptanceContractMet,
    LeaseExpired,
    RetryAllowed,
    RecoveryTriggered,
    UserCancelled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pending_to_ready_valid() {
        assert!(TaskState::Pending.can_transition_to(TaskState::Ready).is_ok());
    }

    #[test]
    fn test_running_to_succeeded_valid() {
        assert!(TaskState::Running.can_transition_to(TaskState::Succeeded).is_ok());
    }

    #[test]
    fn test_timedout_to_ready_valid() {
        assert!(TaskState::TimedOut.can_transition_to(TaskState::Ready).is_ok());
    }

    #[test]
    fn test_succeeded_is_terminal() {
        assert!(TaskState::Succeeded.is_terminal());
    }

    #[test]
    fn test_pending_to_succeeded_invalid() {
        assert!(TaskState::Pending.can_transition_to(TaskState::Succeeded).is_err());
    }

    #[test]
    fn test_pending_to_running_invalid() {
        assert!(TaskState::Pending.can_transition_to(TaskState::Running).is_err());
    }

    #[test]
    fn test_succeeded_to_any_invalid() {
        for next in &[TaskState::Ready, TaskState::Running, TaskState::Failed] {
            assert!(TaskState::Succeeded.can_transition_to(*next).is_err(),
                "Succeeded -> {:?} should be invalid", next);
        }
    }

    #[test]
    fn test_fencing_token_rejects_stale() {
        let mut task = Task::new(Uuid::now_v7(), Uuid::now_v7(), "test".into());
        task.state = TaskState::Running;
        task.current_fencing_token = 5;
        assert!(task.verify_fencing(4).is_err());
        assert!(task.verify_fencing(5).is_ok());
        assert!(task.verify_fencing(6).is_ok());
    }

    #[test]
    fn test_invalidated_task_cannot_submit() {
        let mut task = Task::new(Uuid::now_v7(), Uuid::now_v7(), "test".into());
        task.state = TaskState::Running;
        task.replan_disposition = ReplanDisposition::Invalidated;
        assert!(task.verify_submissible().is_err());
    }

    #[test]
    fn test_frozen_task_cannot_submit() {
        let mut task = Task::new(Uuid::now_v7(), Uuid::now_v7(), "test".into());
        task.state = TaskState::Running;
        task.replan_disposition = ReplanDisposition::Frozen;
        assert!(task.verify_submissible().is_err());
    }
}

use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum DomainError {
    #[error("version conflict: expected {expected}, actual {actual}")]
    VersionConflict { expected: i64, actual: i64 },

    #[error("command {command_id} already replayed")]
    CommandReplayed { command_id: String },

    #[error("invalid transition: from {from:?} to {to:?}")]
    InvalidTransition { from: String, to: String },

    #[error("lease expired")]
    LeaseExpired,

    #[error("fencing token stale: provided {provided}, current {current}")]
    FencingTokenStale { provided: u64, current: u64 },

    #[error("task not ready")]
    TaskNotReady,

    #[error("session not found: {session_id}")]
    SessionNotFound { session_id: String },

    #[error("task not found: {task_id}")]
    TaskNotFound { task_id: String },

    #[error("tenant not found: {tenant_id}")]
    TenantNotFound { tenant_id: String },

    #[error("quota exceeded: {detail}")]
    QuotaExceeded { detail: String },

    #[error("invalid event: {detail}")]
    InvalidEvent { detail: String },

    #[error("validation failed: {detail}")]
    ValidationFailed { detail: String },
}

pub type DomainResult<T> = Result<T, DomainError>;

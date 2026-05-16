use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::DomainResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApprovalStatus {
    PendingHuman,
    Approved,
    Rejected,
    Expired,
    Revoked,
    Consumed,
}

impl ApprovalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PendingHuman => "pending_human",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Expired => "expired",
            Self::Revoked => "revoked",
            Self::Consumed => "consumed",
        }
    }
}

/// An approval request triggered by sensitive operations (architecture doc section 6)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub approval_request_id: Uuid,
    pub command_id: Uuid,
    pub session_id: Uuid,
    pub task_id: Uuid,
    pub task_attempt: i32,
    pub lease_id: Uuid,
    pub fencing_token: u64,
    pub worker_spec_id: Uuid,
    pub tool_name: String,
    pub patch_risk_level: String,
    pub reason_codes: Vec<String>,
    pub target_paths: Vec<String>,
    pub base_snapshot_id: String,
    pub payload_sha256: String,
    pub status: ApprovalStatus,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub approved_by: Option<String>,
    pub rejected_by: Option<String>,
    pub rejected_reason: Option<String>,
}

impl ApprovalRequest {
    pub fn is_pending(&self) -> bool {
        self.status == ApprovalStatus::PendingHuman
    }

    pub fn is_expired(&self) -> bool {
        self.status == ApprovalStatus::Expired
            || (self.status == ApprovalStatus::PendingHuman && Utc::now() >= self.expires_at)
    }
}

/// Approval Token — a consumable credential binding an approval to specific execution context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalToken {
    pub approval_token_id: Uuid,
    pub approval_request_id: Uuid,
    pub session_id: Uuid,
    pub task_id: Uuid,
    pub task_attempt: i32,
    pub lease_id: Uuid,
    pub fencing_token: u64,
    pub worker_spec_id: Uuid,
    pub tool_name: String,
    pub target_paths: Vec<String>,
    pub base_snapshot_id: String,
    pub payload_sha256: String,
    pub status: ApprovalStatus,
    pub usage_limit: u32,
    pub usage_count: u32,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub approved_by: String,
    pub signature: Vec<u8>,
}

impl ApprovalToken {
    /// Verify that the token matches the current execution context (section 6.7)
    pub fn verify_binding(
        &self,
        session_id: Uuid,
        task_id: Uuid,
        task_attempt: i32,
        lease_id: Uuid,
        fencing_token: u64,
        base_snapshot_id: &str,
        payload_sha256: &str,
    ) -> DomainResult<()> {
        if self.session_id != session_id
            || self.task_id != task_id
            || self.task_attempt != task_attempt
            || self.lease_id != lease_id
            || self.fencing_token != fencing_token
        {
            return Err(crate::DomainError::ValidationFailed {
                detail: "approval token binding mismatch".into(),
            });
        }

        if self.base_snapshot_id != base_snapshot_id {
            return Err(crate::DomainError::ValidationFailed {
                detail: "snapshot mismatch".into(),
            });
        }

        if self.payload_sha256 != payload_sha256 {
            return Err(crate::DomainError::ValidationFailed {
                detail: "payload hash mismatch".into(),
            });
        }

        if self.status != ApprovalStatus::Approved {
            return Err(crate::DomainError::ValidationFailed {
                detail: "token not in approved state".into(),
            });
        }

        if Utc::now() >= self.expires_at {
            return Err(crate::DomainError::ValidationFailed {
                detail: "token expired".into(),
            });
        }

        if self.usage_count >= self.usage_limit {
            return Err(crate::DomainError::ValidationFailed {
                detail: "token already consumed".into(),
            });
        }

        Ok(())
    }

    pub fn has_capacity(&self) -> bool {
        self.usage_count < self.usage_limit
    }
}

/// The full approval binding for authorization (port from v1 proto)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalBinding {
    pub approval_request_id: String,
    pub approval_token_id: String,
    pub session_id: Uuid,
    pub task_id: Uuid,
    pub task_attempt: u32,
    pub lease_id: Uuid,
    pub fencing_token: u64,
    pub worker_spec_id: Uuid,
    pub tool_name: String,
    pub target_paths: Vec<String>,
    pub base_snapshot_id: String,
    pub payload_sha256: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub approved_by: String,
    pub usage_limit: u32,
    pub usage_count: u32,
    pub status: ApprovalStatus,
    pub signature: Vec<u8>,
}

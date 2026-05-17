use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::DomainError;
use crate::tenant::TenantPolicy;
use crate::session_phase::SessionPhase;

/// Session aggregate root (architecture doc section 3)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub session_id: Uuid,
    pub tenant_id: Uuid,
    pub goal: String,
    pub state: SessionState,
    pub current_plan_version: i32,
    pub current_plan_epoch: i32,
    pub session_version: i64,
    pub policy_profile: TenantPolicy,
    #[serde(default)]
    pub current_phase: SessionPhase,
    pub pending_gate_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    Active,
    Completed,
    Aborted,
}

impl SessionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Completed => "completed",
            Self::Aborted => "aborted",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "active" => Some(Self::Active),
            "completed" => Some(Self::Completed),
            "aborted" => Some(Self::Aborted),
            _ => None,
        }
    }
}

impl Session {
    pub fn new(
        session_id: Uuid,
        tenant_id: Uuid,
        goal: String,
        policy_profile: TenantPolicy,
    ) -> Self {
        let now = Utc::now();
        Self {
            session_id,
            tenant_id,
            goal,
            state: SessionState::Active,
            current_plan_version: 0,
            current_plan_epoch: 0,
            session_version: 0,
            policy_profile,
            current_phase: SessionPhase::default(),
            pending_gate_id: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Increment the session version for CAS operations
    pub fn next_version(&self) -> i64 {
        self.session_version + 1
    }

    /// Verify a CAS expected version matches the current session version
    pub fn verify_version(&self, expected: i64) -> Result<(), DomainError> {
        if expected != self.session_version {
            return Err(DomainError::VersionConflict {
                expected,
                actual: self.session_version,
            });
        }
        Ok(())
    }

    /// Advance to Completed
    pub fn complete(&mut self) -> Result<(), DomainError> {
        if self.state != SessionState::Active {
            return Err(DomainError::InvalidTransition {
                from: self.state.as_str().into(),
                to: "completed".into(),
            });
        }
        self.state = SessionState::Completed;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Abort the session
    pub fn abort(&mut self) -> Result<(), DomainError> {
        if self.state != SessionState::Active {
            return Err(DomainError::InvalidTransition {
                from: self.state.as_str().into(),
                to: "aborted".into(),
            });
        }
        self.state = SessionState::Aborted;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Update plan version after a compilation
    pub fn set_plan_version(&mut self, version: i32, epoch: i32) {
        self.current_plan_version = version;
        self.current_plan_epoch = epoch;
        self.updated_at = Utc::now();
    }

    pub fn advance_phase(&mut self, next: SessionPhase) -> Result<(), DomainError> {
        if next == self.current_phase {
            return Err(DomainError::ValidationFailed { detail: "phase unchanged".into() });
        }
        self.current_phase = next;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn set_pending_gate(&mut self, gate_id: Uuid) {
        self.pending_gate_id = Some(gate_id);
        self.updated_at = Utc::now();
    }

    pub fn clear_pending_gate(&mut self) {
        self.pending_gate_id = None;
        self.updated_at = Utc::now();
    }

    pub fn is_at_gate(&self) -> bool {
        self.pending_gate_id.is_some() && self.current_phase.is_gate()
    }
}

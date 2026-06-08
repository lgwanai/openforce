use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Effect type — now a string-based type so that platform operators can register
/// custom effect types beyond the built-in set.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EffectType(String);

impl EffectType {
    // Built-in effect types (architecture doc section 7)
    pub fn deploy() -> Self {
        Self("deploy".into())
    }
    pub fn migration() -> Self {
        Self("migration".into())
    }
    pub fn notification() -> Self {
        Self("notification".into())
    }
    pub fn webhook() -> Self {
        Self("webhook".into())
    }
    pub fn iam_change() -> Self {
        Self("iam_change".into())
    }
    pub fn billing_resource() -> Self {
        Self("billing_resource".into())
    }
    pub fn data_delete() -> Self {
        Self("data_delete".into())
    }

    /// Create a custom effect type from any string
    pub fn custom(name: &str) -> Self {
        Self(name.to_lowercase())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Some(Self(s.into()))
    }
}

impl Default for EffectType {
    fn default() -> Self {
        Self::deploy()
    }
}

impl std::fmt::Display for EffectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique effect identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EffectId(pub Uuid);

impl EffectId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

/// Idempotency key for preventing duplicate side effects (section 7.5)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdempotencyKey(pub String);

impl IdempotencyKey {
    pub fn new(session_id: Uuid, effect_type: &EffectType, target: &str) -> Self {
        Self(format!(
            "{}:{}:{}",
            session_id,
            effect_type.as_str(),
            target
        ))
    }

    pub fn value(&self) -> &str {
        &self.0
    }
}

/// Effect lifecycle states (section 19.7)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectState {
    Requested,
    PendingApproval,
    Approved,
    Dispatching,
    Executing,
    Committed,
    Rejected,
    Compensated,
}

impl EffectState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::PendingApproval => "pending_approval",
            Self::Approved => "approved",
            Self::Dispatching => "dispatching",
            Self::Executing => "executing",
            Self::Committed => "committed",
            Self::Rejected => "rejected",
            Self::Compensated => "compensated",
        }
    }
}

impl EffectState {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "requested" => Some(Self::Requested),
            "pending_approval" => Some(Self::PendingApproval),
            "approved" => Some(Self::Approved),
            "dispatching" => Some(Self::Dispatching),
            "executing" => Some(Self::Executing),
            "committed" => Some(Self::Committed),
            "rejected" => Some(Self::Rejected),
            "compensated" => Some(Self::Compensated),
            _ => None,
        }
    }
}

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Effect types that must go through Effect Gateway (architecture doc section 7)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EffectType {
    Deploy,
    Migration,
    Notification,
    Webhook,
    IamChange,
    BillingResource,
    DataDelete,
}

impl EffectType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Deploy => "deploy",
            Self::Migration => "migration",
            Self::Notification => "notification",
            Self::Webhook => "webhook",
            Self::IamChange => "iam_change",
            Self::BillingResource => "billing_resource",
            Self::DataDelete => "data_delete",
        }
    }
}

/// Unique effect identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EffectId(pub Uuid);

impl EffectId {
    pub fn new() -> Self { Self(Uuid::now_v7()) }
}

/// Idempotency key for preventing duplicate side effects (section 7.5)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdempotencyKey(pub String);

impl IdempotencyKey {
    pub fn new(session_id: Uuid, effect_type: &EffectType, target: &str) -> Self {
        Self(format!("{}:{}:{}", session_id, effect_type.as_str(), target))
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

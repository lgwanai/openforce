use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// SessionPhase is now a string-based type so that pipeline authors can define
/// arbitrary phases beyond the built-in software-engineering lifecycle.
/// The built-in phases are provided as constants for backward compatibility,
/// but `SessionPhase::from_str()` accepts any string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionPhase(String);

// Built-in phase constants for the default software-engineering pipeline
impl SessionPhase {
    pub fn understand() -> Self { Self("understand".into()) }
    pub fn design() -> Self { Self("design".into()) }
    pub fn confirm_design() -> Self { Self("confirm_design".into()) }
    pub fn architecture() -> Self { Self("architecture".into()) }
    pub fn development() -> Self { Self("development".into()) }
    pub fn confirm_dev() -> Self { Self("confirm_dev".into()) }
    pub fn test() -> Self { Self("test".into()) }
    pub fn fix() -> Self { Self("fix".into()) }
    pub fn confirm_final() -> Self { Self("confirm_final".into()) }
    pub fn report() -> Self { Self("report".into()) }
    pub fn complete() -> Self { Self("complete".into()) }

    /// Create a custom phase from any string
    pub fn custom(name: &str) -> Self { Self(name.to_lowercase()) }

    pub fn as_str(&self) -> &str { &self.0 }

    pub fn from_str(s: &str) -> Option<Self> { Some(Self(s.to_lowercase())) }

    /// Whether this phase is a confirmation gate (starts with "confirm_")
    pub fn is_gate(&self) -> bool { self.0.starts_with("confirm_") || self.0 == "gate" }

    pub fn is_terminal(&self) -> bool { self.0 == "complete" }

    /// Returns the next phase in the default built-in pipeline.
    /// Returns `None` for custom phases or terminal phases — the pipeline
    /// configuration must supply the transition in those cases.
    pub fn next_phase(&self) -> Option<Self> {
        match self.0.as_str() {
            "understand"     => Some(Self::design()),
            "design"         => Some(Self::confirm_design()),
            "confirm_design" => Some(Self::architecture()),
            "architecture"   => Some(Self::development()),
            "development"    => Some(Self::confirm_dev()),
            "confirm_dev"    => Some(Self::test()),
            "test"           => Some(Self::fix()),
            "fix"            => Some(Self::confirm_final()),
            "confirm_final"  => Some(Self::report()),
            "report"         => Some(Self::complete()),
            _ => None, // custom or terminal phases
        }
    }

    pub fn description(&self) -> &str {
        match self.0.as_str() {
            "understand" => "Reading and analyzing project structure",
            "design" => "Creating design specifications",
            "confirm_design" => "Reviewing design — user confirmation required",
            "architecture" => "Designing system architecture",
            "development" => "Implementing code",
            "confirm_dev" => "Reviewing implementation — user confirmation required",
            "test" => "Running tests",
            "fix" => "Fixing issues found during testing",
            "confirm_final" => "Final review — user confirmation required",
            "report" => "Generating final report",
            "complete" => "Session complete",
            other => other,
        }
    }
}

impl Default for SessionPhase {
    fn default() -> Self { Self::understand() }
}

impl std::fmt::Display for SessionPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.0) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GateStatus {
    Pending,
    Approved,
    Rejected,
}

impl GateStatus {
    pub fn as_str(&self) -> &'static str {
        match self { Self::Pending => "pending", Self::Approved => "approved", Self::Rejected => "rejected" }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmationGate {
    pub gate_id: Uuid,
    pub session_id: Uuid,
    pub phase: SessionPhase,
    pub status: GateStatus,
    pub user_feedback: Option<String>,
    pub artifact_summary: Option<String>,
    pub plan_epoch: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl ConfirmationGate {
    pub fn new(session_id: Uuid, phase: SessionPhase, artifact_summary: String, plan_epoch: i32) -> Self {
        Self {
            gate_id: Uuid::now_v7(), session_id, phase, status: GateStatus::Pending,
            user_feedback: None, artifact_summary: Some(artifact_summary), plan_epoch,
            created_at: chrono::Utc::now(), resolved_at: None,
        }
    }

    pub fn approve(&mut self) {
        self.status = GateStatus::Approved;
        self.resolved_at = Some(chrono::Utc::now());
    }

    pub fn reject(&mut self, feedback: String) {
        self.status = GateStatus::Rejected;
        self.user_feedback = Some(feedback);
        self.resolved_at = Some(chrono::Utc::now());
    }

    pub fn is_pending(&self) -> bool { matches!(self.status, GateStatus::Pending) }
}

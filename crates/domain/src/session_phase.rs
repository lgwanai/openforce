use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionPhase {
    Understand,
    Design,
    ConfirmDesign,
    Architecture,
    Development,
    ConfirmDev,
    Test,
    Fix,
    ConfirmFinal,
    Report,
    Complete,
}

impl SessionPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Understand => "understand",
            Self::Design => "design",
            Self::ConfirmDesign => "confirm_design",
            Self::Architecture => "architecture",
            Self::Development => "development",
            Self::ConfirmDev => "confirm_dev",
            Self::Test => "test",
            Self::Fix => "fix",
            Self::ConfirmFinal => "confirm_final",
            Self::Report => "report",
            Self::Complete => "complete",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "understand" => Some(Self::Understand),
            "design" => Some(Self::Design),
            "confirm_design" => Some(Self::ConfirmDesign),
            "architecture" => Some(Self::Architecture),
            "development" => Some(Self::Development),
            "confirm_dev" => Some(Self::ConfirmDev),
            "test" => Some(Self::Test),
            "fix" => Some(Self::Fix),
            "confirm_final" => Some(Self::ConfirmFinal),
            "report" => Some(Self::Report),
            "complete" => Some(Self::Complete),
            _ => None,
        }
    }

    pub fn next_phase(&self) -> Option<SessionPhase> {
        match self {
            Self::Understand => Some(Self::Design),
            Self::Design => Some(Self::ConfirmDesign),
            Self::ConfirmDesign => Some(Self::Architecture),
            Self::Architecture => Some(Self::Development),
            Self::Development => Some(Self::ConfirmDev),
            Self::ConfirmDev => Some(Self::Test),
            Self::Test => Some(Self::Fix),
            Self::Fix => Some(Self::Test),
            Self::ConfirmFinal => Some(Self::Report),
            Self::Report => Some(Self::Complete),
            Self::Complete => None,
        }
    }

    pub fn is_gate(&self) -> bool {
        matches!(self, Self::ConfirmDesign | Self::ConfirmDev | Self::ConfirmFinal)
    }

    pub fn is_terminal(&self) -> bool { matches!(self, Self::Complete) }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Understand => "Reading and analyzing project structure",
            Self::Design => "Creating design specifications",
            Self::ConfirmDesign => "Reviewing design — user confirmation required",
            Self::Architecture => "Designing system architecture",
            Self::Development => "Implementing code",
            Self::ConfirmDev => "Reviewing implementation — user confirmation required",
            Self::Test => "Running tests",
            Self::Fix => "Fixing issues found during testing",
            Self::ConfirmFinal => "Final review — user confirmation required",
            Self::Report => "Generating final report",
            Self::Complete => "Session complete",
        }
    }
}

impl Default for SessionPhase {
    fn default() -> Self { Self::Understand }
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

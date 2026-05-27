use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Phase groups for progressive planning.
///
/// Design phases must all PASS before Implementation phases begin.
/// The Planner RoundTable plans one group at a time:
/// - **Design group**: tasks planned in full detail (concrete).
/// - **Implementation group**: tasks sketched as placeholders until the design
///   gate is approved, then replanned in detail with full architecture context.
/// - **Report group**: wrap-up tasks planned in detail.
///
/// This enforces the principle that far-term tasks are inherently fuzzy —
/// architecture must be complete before development tasks can be specified precisely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PhaseGroup {
    /// understand → design → confirm_design → architecture
    Design,
    /// development → confirm_dev → test → fix → confirm_final
    Implementation,
    /// report → complete
    Report,
}

impl PhaseGroup {
    pub fn as_str(&self) -> &'static str {
        match self {
            PhaseGroup::Design => "design",
            PhaseGroup::Implementation => "implementation",
            PhaseGroup::Report => "report",
        }
    }

}

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

    /// Returns which phase group this phase belongs to.
    /// Used by the Planner to scope RoundTable planning to the current group.
    pub fn phase_group(&self) -> PhaseGroup {
        match self.0.as_str() {
            "understand" | "design" | "confirm_design" | "architecture" => PhaseGroup::Design,
            "development" | "confirm_dev" | "test" | "fix" | "confirm_final" => PhaseGroup::Implementation,
            "report" | "complete" => PhaseGroup::Report,
            // Custom phases default to Design (conservative: plan in detail)
            _ => PhaseGroup::Design,
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phase_group_mapping() {
        assert_eq!(SessionPhase::understand().phase_group(), PhaseGroup::Design);
        assert_eq!(SessionPhase::design().phase_group(), PhaseGroup::Design);
        assert_eq!(SessionPhase::confirm_design().phase_group(), PhaseGroup::Design);
        assert_eq!(SessionPhase::architecture().phase_group(), PhaseGroup::Design);

        assert_eq!(SessionPhase::development().phase_group(), PhaseGroup::Implementation);
        assert_eq!(SessionPhase::confirm_dev().phase_group(), PhaseGroup::Implementation);
        assert_eq!(SessionPhase::test().phase_group(), PhaseGroup::Implementation);
        assert_eq!(SessionPhase::fix().phase_group(), PhaseGroup::Implementation);
        assert_eq!(SessionPhase::confirm_final().phase_group(), PhaseGroup::Implementation);

        assert_eq!(SessionPhase::report().phase_group(), PhaseGroup::Report);
        assert_eq!(SessionPhase::complete().phase_group(), PhaseGroup::Report);
    }

    #[test]
    fn test_custom_phase_defaults_to_design() {
        let custom = SessionPhase::custom("deploy_to_staging");
        assert_eq!(custom.phase_group(), PhaseGroup::Design);
    }

    #[test]
    fn test_phase_group_gate_detection() {
        // confirm_design is in Design group and IS a gate
        assert!(SessionPhase::confirm_design().is_gate());
        assert_eq!(SessionPhase::confirm_design().phase_group(), PhaseGroup::Design);

        // confirm_dev is in Implementation group and IS a gate
        assert!(SessionPhase::confirm_dev().is_gate());
        assert_eq!(SessionPhase::confirm_dev().phase_group(), PhaseGroup::Implementation);

        // confirm_final is in Implementation group and IS a gate
        assert!(SessionPhase::confirm_final().is_gate());
        assert_eq!(SessionPhase::confirm_final().phase_group(), PhaseGroup::Implementation);

        // architecture is in Design group but NOT a gate
        assert!(!SessionPhase::architecture().is_gate());
        assert_eq!(SessionPhase::architecture().phase_group(), PhaseGroup::Design);
    }

    #[test]
    fn test_pipeline_transitions_cross_groups() {
        // Design → Implementation transition happens at confirm_design gate
        let design_gate = SessionPhase::confirm_design();
        assert!(design_gate.is_gate());
        let after_design_gate = design_gate.next_phase().unwrap();
        assert_eq!(after_design_gate, SessionPhase::architecture());

        // Implementation group starts at development
        let dev = SessionPhase::development();
        assert_eq!(dev.phase_group(), PhaseGroup::Implementation);

        // Implementation → Report transition
        let impl_gate = SessionPhase::confirm_final();
        assert!(impl_gate.is_gate());
        let after_impl_gate = impl_gate.next_phase().unwrap();
        assert_eq!(after_impl_gate, SessionPhase::report());
        assert_eq!(after_impl_gate.phase_group(), PhaseGroup::Report);
    }
}

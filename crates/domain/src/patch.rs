use serde::{Deserialize, Serialize};

/// Patch semantic risk levels (architecture doc section 6.5)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatchRiskLevel {
    Safe,
    Moderate,
    Sensitive,
    Reject,
}

impl PatchRiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Moderate => "moderate",
            Self::Sensitive => "sensitive",
            Self::Reject => "reject",
        }
    }

    pub fn requires_approval(&self) -> bool {
        matches!(self, Self::Sensitive)
    }

    pub fn is_rejected(&self) -> bool {
        matches!(self, Self::Reject)
    }
}

/// Reason codes for patch classification — now a string-based type so that
/// custom classifiers can produce arbitrary reason codes beyond the built-in set.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PatchReasonCode(String);

impl PatchReasonCode {
    // Built-in reason codes (architecture doc section 6.5, PCR rules)
    pub fn delete_equivalent_patch() -> Self { Self("delete_equivalent_patch".into()) }
    pub fn touches_core_route() -> Self { Self("touches_core_route".into()) }
    pub fn touches_auth_logic() -> Self { Self("touches_auth_logic".into()) }
    pub fn touches_migration() -> Self { Self("touches_migration".into()) }
    pub fn touches_prod_config() -> Self { Self("touches_prod_config".into()) }
    pub fn batch_delete() -> Self { Self("batch_delete".into()) }
    pub fn file_truncation() -> Self { Self("file_truncation".into()) }
    pub fn cross_scope_write() -> Self { Self("cross_scope_write".into()) }
    pub fn rename_with_wide_impact() -> Self { Self("rename_with_wide_impact".into()) }

    /// Create a custom reason code from any string
    pub fn custom(code: &str) -> Self { Self(code.into()) }

    pub fn as_str(&self) -> &str { &self.0 }

    pub fn from_str(s: &str) -> Option<Self> { Some(Self(s.into())) }
}

impl std::fmt::Display for PatchReasonCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{}", self.0) }
}

/// Classification result for a submitted patch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchClassification {
    pub risk_level: PatchRiskLevel,
    pub reason_codes: Vec<PatchReasonCode>,
    pub requires_approval: bool,
}

impl PatchClassification {
    pub fn new(risk_level: PatchRiskLevel, reason_codes: Vec<PatchReasonCode>) -> Self {
        let requires_approval = risk_level.requires_approval();
        Self {
            risk_level,
            reason_codes,
            requires_approval,
        }
    }

    pub fn safe() -> Self {
        Self::new(PatchRiskLevel::Safe, vec![])
    }

    pub fn rejected(reason: PatchReasonCode) -> Self {
        Self::new(PatchRiskLevel::Reject, vec![reason])
    }

    pub fn sensitive(reason: PatchReasonCode) -> Self {
        Self::new(PatchRiskLevel::Sensitive, vec![reason])
    }

    pub fn can_proceed(&self) -> bool {
        !self.risk_level.is_rejected()
    }
}

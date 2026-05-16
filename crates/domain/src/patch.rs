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

/// Reason codes for patch classification (architecture doc section 6.5, PCR rules)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatchReasonCode {
    DeleteEquivalentPatch,
    TouchesCoreRoute,
    TouchesAuthLogic,
    TouchesMigration,
    TouchesProdConfig,
    BatchDelete,
    FileTruncation,
    CrossScopeWrite,
    RenameWithWideImpact,
}

impl PatchReasonCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DeleteEquivalentPatch => "delete_equivalent_patch",
            Self::TouchesCoreRoute => "touches_core_route",
            Self::TouchesAuthLogic => "touches_auth_logic",
            Self::TouchesMigration => "touches_migration",
            Self::TouchesProdConfig => "touches_prod_config",
            Self::BatchDelete => "batch_delete",
            Self::FileTruncation => "file_truncation",
            Self::CrossScopeWrite => "cross_scope_write",
            Self::RenameWithWideImpact => "rename_with_wide_impact",
        }
    }
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

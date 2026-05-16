use serde::{Serialize, Deserialize};
use uuid::Uuid;

/// Evolver produces versioned candidates — never silently mutates production.
/// Architecture doc section 11.2: all updates are versioned, grayscale, rollbackable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactVersion {
    pub artifact_id: Uuid,
    pub artifact_type: ArtifactType,
    pub version: u32,
    pub sha256: String,
    pub status: VersionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArtifactType {
    PromptBundle,
    RoleProfile,
    ToolPolicy,
    ApprovalPolicy,
    EvaluatorRuleSet,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VersionStatus {
    Candidate,
    Evaluating,
    CanaryActive,
    Promoted,
    Frozen,
    RolledBack,
}

pub struct Evolver;

impl Evolver {
    pub fn create_candidate(artifact_type: ArtifactType, current_version: u32, sha256: &str) -> ArtifactVersion {
        ArtifactVersion {
            artifact_id: Uuid::now_v7(), artifact_type, version: current_version + 1,
            sha256: sha256.into(), status: VersionStatus::Candidate,
        }
    }

    pub fn promote(candidate: &mut ArtifactVersion) { candidate.status = VersionStatus::Promoted; }
    pub fn freeze(candidate: &mut ArtifactVersion) { candidate.status = VersionStatus::Frozen; }
    pub fn rollback(candidate: &mut ArtifactVersion) { candidate.status = VersionStatus::RolledBack; }

    pub fn is_safe_to_use(status: &VersionStatus) -> bool {
        matches!(status, VersionStatus::Promoted | VersionStatus::CanaryActive)
    }
}

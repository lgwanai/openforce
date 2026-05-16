use crate::evolver::{ArtifactVersion, VersionStatus};
use uuid::Uuid;

/// Canary release with A/B testing support.
/// Architecture doc section 11.2: grayscale release, rollback on regression.
pub struct CanaryRelease {
    pub canary_id: Uuid,
    pub stable_version: ArtifactVersion,
    pub canary_version: ArtifactVersion,
    pub traffic_split_pct: u8,  // percentage going to canary
    pub active: bool,
}

impl CanaryRelease {
    pub fn new(stable: ArtifactVersion, canary: ArtifactVersion, split_pct: u8) -> Self {
        Self { canary_id: Uuid::now_v7(), stable_version: stable, canary_version: canary, traffic_split_pct: split_pct, active: true }
    }

    pub fn promote_canary(&mut self, mut canary: ArtifactVersion) {
        canary.status = VersionStatus::Promoted;
        self.canary_version = canary;
        self.active = false;
    }

    pub fn rollback(&mut self, mut canary: ArtifactVersion) -> ArtifactVersion {
        canary.status = VersionStatus::RolledBack;
        self.active = false;
        canary
    }

    pub fn route_to_canary(&self, request_id: Uuid) -> bool {
        if !self.active { return false; }
        // Deterministic routing based on request_id for consistent A/B
        let bytes = request_id.as_bytes();
        let bucket = bytes[0] as u16;
        (bucket % 100) < self.traffic_split_pct as u16
    }
}

use openforce_domain::patch::{PatchClassification, PatchRiskLevel, PatchReasonCode};

/// PatchClassifier implements the 9 semantic risk classification rules
/// from architecture doc section 6.5 (PCR-001 through PCR-009).
pub struct PatchClassifier {
    sensitive_patterns: Vec<String>,
}

impl Default for PatchClassifier {
    fn default() -> Self {
        Self {
            sensitive_patterns: vec![
                "**/migrations/**".into(),
                "**/infra/prod/**".into(),
                "**/auth/**".into(),
                "**/route*".into(),
                "**/config*.{yaml,json,toml,env}".into(),
                "**/secrets/**".into(),
                "**/.env*".into(),
                "**/Cargo.lock".into(),
            ],
        }
    }
}

impl PatchClassifier {
    pub fn new() -> Self { Self::default() }

    /// Classify a patch based on target paths and diff summary.
    /// Returns a classification with risk level and reason codes.
    pub fn classify(
        &self,
        target_paths: &[String],
        allowed_read_paths: &[String],
        allowed_write_paths: &[String],
        forbidden_paths: &[String],
        lines_removed: usize,
        lines_added: usize,
        files_deleted: usize,
    ) -> PatchClassification {
        let mut reasons = vec![];

        // PCR-001: Cross-scope write detection
        for path in target_paths {
            if !Self::path_in_scope(path, allowed_write_paths) && !allowed_write_paths.is_empty() {
                reasons.push(PatchReasonCode::CrossScopeWrite);
                break;
            }
        }

        // PCR-002: Forbidden path touches
        for path in target_paths {
            if Self::path_matches_any(path, forbidden_paths) {
                reasons.push(PatchReasonCode::CrossScopeWrite);
                return PatchClassification::rejected(PatchReasonCode::CrossScopeWrite);
            }
        }

        // PCR-003: Delete equivalent (mass removal)
        if files_deleted > 0 || (lines_removed > 0 && lines_added == 0) {
            reasons.push(PatchReasonCode::DeleteEquivalentPatch);
        }

        // PCR-004: Batch delete detection
        if files_deleted >= 3 {
            reasons.push(PatchReasonCode::BatchDelete);
        }

        // PCR-005: Touches sensitive areas
        for path in target_paths {
            if self.is_sensitive_path(path) {
                reasons.push(PatchReasonCode::TouchesProdConfig);
                break;
            }
        }

        // PCR-006: File truncation (lots of removal, no adds)
        if lines_removed > 50 && lines_added == 0 {
            reasons.push(PatchReasonCode::FileTruncation);
        }

        // PCR-007: Touches auth/migration
        for path in target_paths {
            if path.contains("auth/") || path.contains("authenticate") {
                reasons.push(PatchReasonCode::TouchesAuthLogic);
            }
            if path.contains("migration") || path.contains("migrate") {
                reasons.push(PatchReasonCode::TouchesMigration);
            }
        }

        // PCR-008: Core route modification
        for path in target_paths {
            if path.contains("route") || path.contains("router") {
                reasons.push(PatchReasonCode::TouchesCoreRoute);
            }
        }

        // PCR-009: Rename with wide impact (proxied by file count)
        if target_paths.len() >= 5 {
            reasons.push(PatchReasonCode::RenameWithWideImpact);
        }

        // Determine overall risk level
        let risk = Self::determine_risk(&reasons);
        PatchClassification::new(risk, reasons)
    }

    fn determine_risk(reasons: &[PatchReasonCode]) -> PatchRiskLevel {
        let has_reject = reasons.iter().any(|r| matches!(r, PatchReasonCode::CrossScopeWrite));
        if has_reject { return PatchRiskLevel::Reject; }

        let has_sensitive = reasons.iter().any(|r| matches!(r,
            PatchReasonCode::DeleteEquivalentPatch
            | PatchReasonCode::TouchesAuthLogic
            | PatchReasonCode::TouchesMigration
            | PatchReasonCode::BatchDelete
            | PatchReasonCode::TouchesProdConfig
        ));
        if has_sensitive { return PatchRiskLevel::Sensitive; }

        let has_moderate = reasons.iter().any(|r| matches!(r,
            PatchReasonCode::TouchesCoreRoute
            | PatchReasonCode::FileTruncation
            | PatchReasonCode::RenameWithWideImpact
        ));
        if has_moderate { return PatchRiskLevel::Moderate; }

        PatchRiskLevel::Safe
    }

    fn is_sensitive_path(&self, path: &str) -> bool {
        self.sensitive_patterns.iter().any(|p| {
            glob::Pattern::new(p).map(|pat| pat.matches(path)).unwrap_or(false)
        })
    }

    fn path_in_scope(path: &str, allowed: &[String]) -> bool {
        if allowed.is_empty() { return true; }
        allowed.iter().any(|p| {
            glob::Pattern::new(p).map(|pat| pat.matches(path)).unwrap_or(false)
        })
    }

    fn path_matches_any(path: &str, patterns: &[String]) -> bool {
        patterns.iter().any(|p| {
            glob::Pattern::new(p).map(|pat| pat.matches(path)).unwrap_or(false)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_patch() {
        let c = PatchClassifier::new();
        let result = c.classify(
            &["frontend/src/components/button.tsx".into()],
            &[], &[], &[], 3, 10, 0,
        );
        assert_eq!(result.risk_level, PatchRiskLevel::Safe);
    }

    #[test]
    fn test_delete_equivalent_detected() {
        let c = PatchClassifier::new();
        let result = c.classify(
            &["src/old_file.rs".into()],
            &[], &[], &[], 100, 0, 0,
        );
        assert_eq!(result.risk_level, PatchRiskLevel::Sensitive);
        assert!(result.reason_codes.contains(&PatchReasonCode::DeleteEquivalentPatch));
    }

    #[test]
    fn test_auth_touch_is_sensitive() {
        let c = PatchClassifier::new();
        let result = c.classify(
            &["backend/auth/service.go".into()],
            &[], &[], &[], 5, 10, 0,
        );
        assert_eq!(result.risk_level, PatchRiskLevel::Sensitive);
        assert!(result.reason_codes.contains(&PatchReasonCode::TouchesAuthLogic));
    }

    #[test]
    fn test_forbidden_path_is_reject() {
        let c = PatchClassifier::new();
        let result = c.classify(
            &["infra/prod/deploy.yaml".into()],
            &[], &[], &["infra/prod/**".into()], 1, 1, 0,
        );
        assert_eq!(result.risk_level, PatchRiskLevel::Reject);
    }
}

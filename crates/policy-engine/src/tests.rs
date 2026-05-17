#[cfg(test)]
mod tests {
    use crate::engine::{AuthzContext, PolicyEngine};
    use crate::rules::*;
    use openforce_domain::identity::{CertificateIdentity, ServiceRole};
    use openforce_domain::token::{CapabilityToken, TokenScope};
    use uuid::Uuid;

    fn test_token(tenant: Uuid, session: Uuid, task: Uuid, lease: Uuid, fencing: u64, scopes: Vec<TokenScope>) -> CapabilityToken {
        CapabilityToken {
            iss: "test".into(), sub: "test".into(),
            tenant_id: tenant, session_id: session, task_id: task,
            task_attempt: 1, lease_id: lease, fencing_token: fencing,
            plan_epoch: 1, scope: scopes,
            jti: Uuid::now_v7(), token_epoch: 1,
            exp: chrono::Utc::now() + chrono::Duration::seconds(300),
            signature: vec![],
        }
    }

    fn ct_identity(role: ServiceRole) -> CertificateIdentity {
        CertificateIdentity {
            role, instance_id: "test-01".into(), region: "us-east-1".into(),
            spiffe_id: format!("spiffe://swarmos.internal/{}/test-01", role.as_str()),
            not_before: chrono::Utc::now(), not_after: chrono::Utc::now() + chrono::Duration::days(30),
        }
    }

    #[test] fn test_default_deny() {
        let engine = PolicyEngine::with_defaults();
        let ctx = AuthzContext {
            mTLS_identity: None, capability_token: None,
            requested_action: "unknown".into(), resource_type: "unknown".into(),
            tenant_id: None, session_id: None, task_id: None, lease_id: None, current_fencing_token: None,
        };
        assert!(matches!(engine.evaluate(&ctx), PolicyEffect::Deny { .. }));
    }

    #[test] fn test_scheduler_lease_allowed() {
        let engine = PolicyEngine::with_defaults();
        let ctx = AuthzContext {
            mTLS_identity: Some(ct_identity(ServiceRole::Scheduler)), capability_token: None,
            requested_action: "LeaseTask".into(), resource_type: "Task".into(),
            tenant_id: None, session_id: None, task_id: None, lease_id: None, current_fencing_token: None,
        };
        assert!(matches!(engine.evaluate(&ctx), PolicyEffect::Allow { .. }));
    }

    #[test] fn test_worker_with_token_allowed() {
        let tid = Uuid::now_v7(); let sid = Uuid::now_v7(); let tid2 = Uuid::now_v7(); let lid = Uuid::now_v7();
        let engine = PolicyEngine::with_defaults();
        let ctx = AuthzContext {
            mTLS_identity: Some(ct_identity(ServiceRole::Worker)),
            capability_token: Some(test_token(tid, sid, tid2, lid, 5, vec![TokenScope::ArtifactSubmit])),
            requested_action: "SubmitArtifact".into(), resource_type: "Artifact".into(),
            tenant_id: Some(tid), session_id: Some(sid), task_id: Some(tid2),
            lease_id: Some(lid), current_fencing_token: Some(5),
        };
        assert!(matches!(engine.evaluate(&ctx), PolicyEffect::Allow { .. }));
    }

    #[test] fn test_worker_without_token_denied() {
        let engine = PolicyEngine::with_defaults();
        let ctx = AuthzContext {
            mTLS_identity: Some(ct_identity(ServiceRole::Worker)), capability_token: None,
            requested_action: "SubmitArtifact".into(), resource_type: "Artifact".into(),
            tenant_id: None, session_id: None, task_id: None, lease_id: None, current_fencing_token: None,
        };
        assert!(matches!(engine.evaluate(&ctx), PolicyEffect::Deny { .. }));
    }

    #[test] fn test_stale_fencing_token_denied() {
        let tid = Uuid::now_v7(); let sid = Uuid::now_v7(); let tid2 = Uuid::now_v7(); let lid = Uuid::now_v7();
        let engine = PolicyEngine::with_defaults();
        let ctx = AuthzContext {
            mTLS_identity: Some(ct_identity(ServiceRole::Worker)),
            capability_token: Some(test_token(tid, sid, tid2, lid, 3, vec![TokenScope::ArtifactSubmit])),
            requested_action: "SubmitArtifact".into(), resource_type: "Artifact".into(),
            tenant_id: Some(tid), session_id: Some(sid), task_id: Some(tid2),
            lease_id: Some(lid), current_fencing_token: Some(5),
        };
        assert!(matches!(engine.evaluate(&ctx), PolicyEffect::Deny { .. }));
    }

    #[test] fn test_node_daemon_spawn_allowed() {
        let engine = PolicyEngine::with_defaults();
        let ctx = AuthzContext {
            mTLS_identity: Some(ct_identity(ServiceRole::NodeDaemon)), capability_token: None,
            requested_action: "SpawnWorker".into(), resource_type: "Resource".into(),
            tenant_id: None, session_id: None, task_id: None, lease_id: None, current_fencing_token: None,
        };
        assert!(matches!(engine.evaluate(&ctx), PolicyEffect::Allow { .. }));
    }

    #[test] fn test_wrong_role_for_action_denied() {
        let engine = PolicyEngine::with_defaults();
        let ctx = AuthzContext {
            mTLS_identity: Some(ct_identity(ServiceRole::Worker)), capability_token: None,
            requested_action: "LeaseTask".into(), resource_type: "Task".into(),
            tenant_id: None, session_id: None, task_id: None, lease_id: None, current_fencing_token: None,
        };
        assert!(matches!(engine.evaluate(&ctx), PolicyEffect::Deny { .. }));
    }

    #[test] fn test_custom_always_allow_rule() {
        let custom = PolicyRule::new("always", 200, RuleCondition::Always,
            PolicyEffect::Allow { reason: "test".into() }, "test");
        let engine = PolicyEngine::new(vec![custom]);
        let ctx = AuthzContext {
            mTLS_identity: None, capability_token: None,
            requested_action: "".into(), resource_type: "".into(),
            tenant_id: None, session_id: None, task_id: None, lease_id: None, current_fencing_token: None,
        };
        assert!(matches!(engine.evaluate(&ctx), PolicyEffect::Allow { .. }));
    }

    #[test] fn test_empty_rules_returns_deny() {
        let engine = PolicyEngine::new(vec![]);
        let ctx = AuthzContext {
            mTLS_identity: None, capability_token: None,
            requested_action: "any".into(), resource_type: "any".into(),
            tenant_id: None, session_id: None, task_id: None, lease_id: None, current_fencing_token: None,
        };
        assert!(matches!(engine.evaluate(&ctx), PolicyEffect::Deny { .. }));
    }

    #[test] fn test_equal_priority_first_match_wins() {
        let allow_first = PolicyRule::new("allow", 100, RuleCondition::Always,
            PolicyEffect::Allow { reason: "first".into() }, "first");
        let deny_second = PolicyRule::new("deny", 100, RuleCondition::Always,
            PolicyEffect::Deny { reason: "second".into() }, "second");
        let engine = PolicyEngine::new(vec![allow_first, deny_second]);
        let ctx = AuthzContext {
            mTLS_identity: None, capability_token: None,
            requested_action: "".into(), resource_type: "".into(),
            tenant_id: None, session_id: None, task_id: None, lease_id: None, current_fencing_token: None,
        };
        assert!(matches!(engine.evaluate(&ctx), PolicyEffect::Allow { .. }));
    }
}

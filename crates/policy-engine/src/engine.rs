use tracing::info;
use crate::rules::{PolicyEffect, PolicyRule, RuleCondition, default_rules};
use openforce_domain::identity::CertificateIdentity;
use openforce_domain::token::CapabilityToken;

/// Context for an authorization decision (architecture doc section 22).
pub struct AuthzContext {
    pub mTLS_identity: Option<CertificateIdentity>,
    pub capability_token: Option<CapabilityToken>,
    pub requested_action: String,
    pub resource_type: String,
    pub tenant_id: Option<uuid::Uuid>,
    pub session_id: Option<uuid::Uuid>,
    pub task_id: Option<uuid::Uuid>,
    pub lease_id: Option<uuid::Uuid>,
    pub current_fencing_token: Option<u64>,
}

/// Three-layer authorization engine.
/// Layer 1: mTLS identity → Layer 2: Capability token → Layer 3: Business conditions.
pub struct PolicyEngine {
    rules: Vec<PolicyRule>,
}

impl PolicyEngine {
    pub fn new(rules: Vec<PolicyRule>) -> Self { Self { rules } }

    pub fn with_defaults() -> Self { Self::new(default_rules()) }

    /// Evaluate authorization context. Rules evaluated in priority order.
    /// First match wins. Default: Deny.
    pub fn evaluate(&self, ctx: &AuthzContext) -> PolicyEffect {
        let mut sorted: Vec<&PolicyRule> = self.rules.iter().collect();
        sorted.sort_by_key(|r| -r.priority);
        for rule in &sorted {
            if self.evaluate_condition(&rule.condition, ctx) {
                info!("policy rule matched: {} → {:?}", rule.rule_id, rule.effect);
                return rule.effect.clone();
            }
        }
        PolicyEffect::Deny { reason: "no matching policy rule".into() }
    }

    fn evaluate_condition(&self, c: &RuleCondition, ctx: &AuthzContext) -> bool {
        match c {
            RuleCondition::RoleIs(role) => ctx.mTLS_identity.as_ref()
                .map(|id| id.role == *role).unwrap_or(false),
            RuleCondition::ActionOn(action, res) =>
                ctx.requested_action == *action && ctx.resource_type == *res,
            RuleCondition::TokenHasScope(scope) => ctx.capability_token.as_ref()
                .map(|t| t.verify_scope(*scope)).unwrap_or(false),
            RuleCondition::FencingTokenValid => match (&ctx.capability_token, ctx.current_fencing_token) {
                (Some(token), Some(current)) => token.fencing_token >= current,
                _ => false,
            },
            RuleCondition::TenantOwnsSession => match (&ctx.tenant_id, &ctx.capability_token) {
                (Some(tid), Some(token)) => token.tenant_id == *tid,
                _ => false,
            },
            RuleCondition::AllOf(conds) => conds.iter().all(|c| self.evaluate_condition(c, ctx)),
            RuleCondition::AnyOf(conds) => conds.iter().any(|c| self.evaluate_condition(c, ctx)),
            RuleCondition::Not(inner) => !self.evaluate_condition(inner, ctx),
            RuleCondition::Always => true,
        }
    }
}

use serde::{Deserialize, Serialize};
use openforce_domain::token::TokenScope;
use openforce_domain::identity::ServiceRole;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyEffect {
    Allow { reason: String },
    Deny { reason: String },
    Escalate { reason: String, approval_url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    pub rule_id: String,
    pub priority: i32,
    pub condition: RuleCondition,
    pub effect: PolicyEffect,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleCondition {
    RoleIs(ServiceRole),
    ActionOn(String, String),
    TokenHasScope(TokenScope),
    FencingTokenValid,
    TenantOwnsSession,
    AllOf(Vec<RuleCondition>),
    AnyOf(Vec<RuleCondition>),
    Not(Box<RuleCondition>),
    Always,
}

impl PolicyRule {
    pub fn new(rule_id: &str, priority: i32, condition: RuleCondition, effect: PolicyEffect, description: &str) -> Self {
        Self { rule_id: rule_id.into(), priority, condition, effect, description: description.into() }
    }
}

pub fn default_rules() -> Vec<PolicyRule> {
    vec![
        PolicyRule::new("scheduler_lease_task", 100,
            RuleCondition::AllOf(vec![
                RuleCondition::RoleIs(ServiceRole::Scheduler),
                RuleCondition::ActionOn("LeaseTask".into(), "Task".into()),
            ]),
            PolicyEffect::Allow { reason: "scheduler authorized".into() },
            "Scheduler is authorized to issue leases and capability tokens"),
        PolicyRule::new("worker_submit_artifact", 90,
            RuleCondition::AllOf(vec![
                RuleCondition::RoleIs(ServiceRole::Worker),
                RuleCondition::ActionOn("SubmitArtifact".into(), "Artifact".into()),
                RuleCondition::TokenHasScope(TokenScope::ArtifactSubmit),
                RuleCondition::FencingTokenValid,
                RuleCondition::TenantOwnsSession,
            ]),
            PolicyEffect::Allow { reason: "worker authorized with token".into() },
            "Worker with valid capability token can submit artifacts"),
        PolicyRule::new("node_daemon_spawn", 100,
            RuleCondition::AllOf(vec![
                RuleCondition::RoleIs(ServiceRole::NodeDaemon),
                RuleCondition::ActionOn("SpawnWorker".into(), "Resource".into()),
            ]),
            PolicyEffect::Allow { reason: "node daemon authorized".into() },
            "Node Daemon is authorized to spawn worker VMs"),
        PolicyRule::new("worker_effect_request", 85,
            RuleCondition::AllOf(vec![
                RuleCondition::RoleIs(ServiceRole::Worker),
                RuleCondition::ActionOn("RequestEffect".into(), "Effect".into()),
                RuleCondition::TokenHasScope(TokenScope::EffectRequest),
                RuleCondition::FencingTokenValid,
            ]),
            PolicyEffect::Allow { reason: "worker effect authorized".into() },
            "Worker can request side effects with capability token"),
        PolicyRule::new("worker_submit_patch", 85,
            RuleCondition::AllOf(vec![
                RuleCondition::RoleIs(ServiceRole::Worker),
                RuleCondition::ActionOn("SubmitPatch".into(), "Patch".into()),
                RuleCondition::TokenHasScope(TokenScope::PatchSubmit),
                RuleCondition::FencingTokenValid,
            ]),
            PolicyEffect::Allow { reason: "worker patch authorized".into() },
            "Worker with valid token can submit patches"),
    ]
}

use openforce_domain::tenant::TenantPolicy;
pub struct ModelEgressPolicy;
impl ModelEgressPolicy {
    pub fn allow(policy: &TenantPolicy, provider: &str, _model: &str) -> bool {
        matches!(policy.model_egress_policy.as_str(), "any" | "private-only")
    }
}

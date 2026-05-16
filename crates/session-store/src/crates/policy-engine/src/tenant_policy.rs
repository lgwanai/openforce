use openforce_domain::tenant::TenantPolicy;
pub struct TenantPolicyEngine;
impl TenantPolicyEngine {
    pub fn allow_model_call(policy: &TenantPolicy, model_endpoint: &str) -> bool {
        matches!(policy.model_egress_policy.as_str(), "any" | "private-only")
    }
    pub fn allow_training(policy: &TenantPolicy) -> bool { policy.training_opt_in }
    pub fn allow_observer_sampling(policy: &TenantPolicy) -> bool {
        matches!(policy.observer_sampling_policy.as_str(), "full" | "aggregated_only")
    }
    pub fn allow_cross_region(policy: &TenantPolicy, _src: &str, _dst: &str) -> bool {
        matches!(policy.cross_region_transfer_policy.as_str(), "any" | "allowed-regions")
    }
}

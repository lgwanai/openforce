use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TenantId(pub uuid::Uuid);

impl TenantId {
    pub fn new() -> Self { Self(uuid::Uuid::now_v7()) }
}

impl std::fmt::Display for TenantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<uuid::Uuid> for TenantId {
    fn from(id: uuid::Uuid) -> Self { Self(id) }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantPolicy {
    pub model_egress_policy: String,
    pub training_opt_in: bool,
    pub log_retention_days: u32,
    pub observer_sampling_policy: String,
    pub cross_region_transfer_policy: String,
    pub byok_policy: String,
}

impl Default for TenantPolicy {
    fn default() -> Self {
        Self {
            model_egress_policy: "private-only".into(),
            training_opt_in: false,
            log_retention_days: 365,
            observer_sampling_policy: "aggregated_only".into(),
            cross_region_transfer_policy: "same-region-only".into(),
            byok_policy: "optional".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantQuota {
    pub max_concurrent_sessions: u32,
    pub max_running_workers: u32,
    pub max_requests_per_minute: u32,
    pub max_monthly_cost_usd: f64,
    pub max_gpu_minutes: u64,
    pub max_effects_per_hour: u32,
}

impl Default for TenantQuota {
    fn default() -> Self {
        Self {
            max_concurrent_sessions: 100,
            max_running_workers: 50,
            max_requests_per_minute: 1000,
            max_monthly_cost_usd: 10000.0,
            max_gpu_minutes: 5000,
            max_effects_per_hour: 100,
        }
    }
}

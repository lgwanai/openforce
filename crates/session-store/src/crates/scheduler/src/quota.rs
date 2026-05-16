use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct TenantQuotaTracker {
    limits: HashMap<Uuid, QuotaLimits>,
    usage: HashMap<Uuid, QuotaUsage>,
}

#[derive(Debug, Clone)]
pub struct QuotaLimits {
    pub max_concurrent_sessions: u32,
    pub max_running_workers: u32,
}

#[derive(Debug, Clone, Default)]
pub struct QuotaUsage {
    pub active_sessions: u32,
    pub running_workers: u32,
}

impl TenantQuotaTracker {
    pub fn new() -> Self { Self { limits: HashMap::new(), usage: HashMap::new() } }
    pub fn set_limit(&mut self, tenant: Uuid, limit: QuotaLimits) {
        self.limits.insert(tenant, limit);
    }
    pub fn can_start_session(&self, tenant: Uuid) -> bool {
        let limit = self.limits.get(&tenant).map(|l| l.max_concurrent_sessions).unwrap_or(u32::MAX);
        let used = self.usage.get(&tenant).map(|u| u.active_sessions).unwrap_or(0);
        used < limit
    }
    pub fn can_launch_worker(&self, tenant: Uuid) -> bool {
        let limit = self.limits.get(&tenant).map(|l| l.max_running_workers).unwrap_or(u32::MAX);
        let used = self.usage.get(&tenant).map(|u| u.running_workers).unwrap_or(0);
        used < limit
    }
}

use std::collections::HashMap; use uuid::Uuid;
#[derive(Debug, Clone)] pub struct QuotaLimits { pub max_concurrent_sessions: u32, pub max_running_workers: u32 }
#[derive(Debug, Clone, Default)] pub struct QuotaUsage { pub active_sessions: u32, pub running_workers: u32 }
pub struct TenantQuotaTracker { limits: HashMap<Uuid, QuotaLimits>, usage: HashMap<Uuid, QuotaUsage> }
impl TenantQuotaTracker {
    pub fn new() -> Self { Self { limits: HashMap::new(), usage: HashMap::new() } }
    pub fn can_start_session(&self, tenant: Uuid) -> bool {
        self.usage.get(&tenant).map_or(true, |u| u.active_sessions < self.limits.get(&tenant).map_or(u32::MAX, |l| l.max_concurrent_sessions))
    }
}

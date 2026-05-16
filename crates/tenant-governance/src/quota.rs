use std::collections::HashMap;
use uuid::Uuid;

/// Fair scheduling with tenant quotas to prevent noisy neighbor (architecture doc section 27.5-27.6)
#[derive(Debug, Clone)]
pub struct TenantWeight {
    pub tenant_id: Uuid,
    pub weight: f64,
    pub active_sessions: u32,
    pub running_workers: u32,
}

impl TenantWeight {
    pub fn deficit_weighted_share(&self, total_weight: f64) -> f64 {
        if total_weight == 0.0 { return 0.0; }
        self.weight / total_weight
    }
}

pub struct FairQuotaScheduler {
    tenants: HashMap<Uuid, TenantWeight>,
}

impl FairQuotaScheduler {
    pub fn new() -> Self { Self { tenants: HashMap::new() } }

    pub fn register(&mut self, tenant_id: Uuid, weight: f64) {
        self.tenants.insert(tenant_id, TenantWeight {
            tenant_id, weight, active_sessions: 0, running_workers: 0,
        });
    }

    pub fn can_schedule(&self, tenant_id: Uuid, max_concurrent: u32) -> bool {
        if let Some(t) = self.tenants.get(&tenant_id) {
            t.running_workers < max_concurrent
        } else { false }
    }

    pub fn pick_next(&self) -> Option<Uuid> {
        let total: f64 = self.tenants.values().map(|t| t.weight).sum();
        self.tenants.values()
            .min_by(|a, b| {
                let sa = a.deficit_weighted_share(total) / (a.running_workers.max(1) as f64);
                let sb = b.deficit_weighted_share(total) / (b.running_workers.max(1) as f64);
                sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|t| t.tenant_id)
    }
}

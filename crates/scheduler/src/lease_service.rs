use chrono::Duration;
use uuid::Uuid;
use openforce_domain::lease::{Lease, LeaseState, DEFAULT_LEASE_TTL_SECS, DEFAULT_HEARTBEAT_INTERVAL_SECS};

pub struct LeaseIssuer { default_ttl: Duration, heartbeat_interval: i32 }
impl Default for LeaseIssuer {
    fn default() -> Self { Self { default_ttl: Duration::seconds(DEFAULT_LEASE_TTL_SECS), heartbeat_interval: DEFAULT_HEARTBEAT_INTERVAL_SECS } }
}
impl LeaseIssuer {
    pub fn issue(&self, sid: Uuid, tid: Uuid, attempt: i32, fencing: u64) -> Lease {
        Lease::new(sid, tid, attempt, fencing, Uuid::now_v7(), self.default_ttl, self.heartbeat_interval)
    }
    pub fn find_expired(leases: &[Lease]) -> Vec<&Lease> {
        leases.iter().filter(|l| l.state == LeaseState::Active && l.is_expired()).collect()
    }
    pub fn next_fencing(current: u64) -> u64 { current + 1 }
}

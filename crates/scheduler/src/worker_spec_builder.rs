use uuid::Uuid;
use openforce_domain::worker_spec::{WorkerSpec, WorkerSpecBuilder};
pub fn build_worker_spec(sid: Uuid, tid: Uuid, ttype: &str, attempt: i32, pv: i32, pe: i32, lease: Uuid, fencing: u64, expire: &str) -> WorkerSpec {
    WorkerSpecBuilder::new(sid, tid, ttype.to_string(), attempt, pv, pe).with_lease(lease, fencing, expire.to_string()).build()
}

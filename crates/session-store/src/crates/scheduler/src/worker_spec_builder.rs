use uuid::Uuid;
use openforce_domain::worker_spec::{WorkerSpec, WorkerSpecBuilder};

pub fn build_worker_spec(
    session_id: Uuid, task_id: Uuid, task_type: &str, task_attempt: i32,
    plan_version: i32, plan_epoch: i32,
    lease_id: Uuid, fencing_token: u64, lease_expire_at: &str,
) -> WorkerSpec {
    WorkerSpecBuilder::new(session_id, task_id, task_type.to_string(), task_attempt, plan_version, plan_epoch)
        .with_lease(lease_id, fencing_token, lease_expire_at.to_string())
        .build()
}

use std::collections::HashMap;
use tokio::time::{interval, Duration};
use uuid::Uuid;
use tracing::{info, warn};
use openforce_domain::task::TaskState;
use openforce_proto::swarmos::v1::{
    session_store_client::SessionStoreClient,
    ExecuteCommandRequest, ListTasksRequest,
    Command as ProtoCommand,
};
use crate::dag::Dag;

pub struct SchedulerRuntime {
    client: SessionStoreClient<tonic::transport::Channel>,
    instance_id: String,
    dag_cache: HashMap<Uuid, Dag>,
    session_ids: Vec<Uuid>,
}

impl SchedulerRuntime {
    pub fn new(client: SessionStoreClient<tonic::transport::Channel>) -> Self {
        Self { client, instance_id: Uuid::now_v7().to_string(), dag_cache: HashMap::new(), session_ids: vec![] }
    }

    pub fn register_session(&mut self, sid: Uuid) { self.session_ids.push(sid); }

    pub async fn run(&mut self) {
        let mut tick = interval(Duration::from_millis(500));
        info!("Scheduler started: strict upstream→downstream ordering enforced");
        loop { tick.tick().await; if let Err(e) = self.tick().await { warn!("tick: {e}"); } }
    }

    async fn tick(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        for sid in &self.session_ids.clone() {
            let tasks = self.fetch_states(*sid).await?;
            if tasks.is_empty() { continue; }
            let dag = self.dag_cache.entry(*sid).or_insert_with(Dag::new);

            // Collect ready tasks first (avoid double borrow on dag)
            let ready = dag.ready_tasks(&tasks);
            let blocked = dag.blocked_tasks(&tasks);
            drop(tasks); // release borrow on self.dag_cache

            for tid in &ready {
                info!("{sid}: {tid} unblocked → Ready");
                self.mark_ready(*sid, *tid).await;
                self.lease(*sid, *tid).await;
            }
            for tid in &blocked {
                warn!("{sid}: {tid} BLOCKED by failed upstream");
            }
        }
        Ok(())
    }

    async fn fetch_states(&mut self, sid: Uuid) -> Result<HashMap<Uuid, TaskState>, Box<dyn std::error::Error>> {
        let resp = self.client.list_tasks(ListTasksRequest { session_id: sid.to_string(), state_filter: String::new() }).await?;
        let mut states = HashMap::new();
        for t in resp.into_inner().tasks {
            if let (Ok(tid), Some(s)) = (Uuid::parse_str(&t.task_id), TaskState::from_str(&t.state)) {
                states.insert(tid, s);
                let ups: Vec<Uuid> = t.upstream_task_ids.iter().filter_map(|id| Uuid::parse_str(id).ok()).collect();
                if !ups.is_empty() { self.dag_cache.entry(sid).or_insert_with(Dag::new).add_node(tid, ups); }
            }
        }
        Ok(states)
    }

    async fn mark_ready(&mut self, sid: Uuid, tid: Uuid) {
        let cmd = ProtoCommand { command_id: Uuid::now_v7().to_string(), command_type: "MarkTaskReady".into(), tenant_id: Uuid::nil().to_string(), session_id: sid.to_string(), task_id: tid.to_string(), expected_version: 0, ..Default::default() };
        let _ = self.client.execute_command(ExecuteCommandRequest { command: Some(cmd) }).await;
    }

    async fn lease(&mut self, sid: Uuid, tid: Uuid) {
        let cmd = ProtoCommand { command_id: Uuid::now_v7().to_string(), command_type: "LeaseTask".into(), tenant_id: Uuid::nil().to_string(), session_id: sid.to_string(), task_id: tid.to_string(), expected_version: 0, ..Default::default() };
        if let Ok(r) = self.client.execute_command(ExecuteCommandRequest { command: Some(cmd) }).await {
            let result: serde_json::Value = serde_json::from_slice(&r.into_inner().result).unwrap_or_default();
            info!("leased {tid}: fencing={}", result.get("fencing_token").and_then(|v| v.as_u64()).unwrap_or(0));
        }
    }
}

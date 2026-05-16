use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};
use uuid::Uuid;
use tracing::{info, warn};

use openforce_domain::command::{Command, CommandType};
use openforce_domain::event::ProducerIdentity;
use openforce_domain::task::TaskState;
use openforce_proto::swarmos::v1::{
    session_store_client::SessionStoreClient,
    ExecuteCommandRequest, ListTasksRequest, GetSessionRequest,
};
use openforce_proto::swarmos::v1::Command as ProtoCommand;

use crate::dag::Dag;
use crate::lease_service::LeaseIssuer;
use crate::retry::RetryPolicy;
use crate::quota::TenantQuotaTracker;
use crate::kill_switch::SchedulerKillSwitch;
use crate::worker_spec_builder;

pub struct SchedulerRuntime {
    client: SessionStoreClient<tonic::transport::Channel>,
    instance_id: String,
    active_sessions: Vec<Uuid>,
    dag_cache: HashMap<Uuid, Dag>,
    retry: RetryPolicy,
    quota: TenantQuotaTracker,
    kill_switch: SchedulerKillSwitch,
}

impl SchedulerRuntime {
    pub fn new(client: SessionStoreClient<tonic::transport::Channel>) -> Self {
        Self {
            client, instance_id: Uuid::now_v7().to_string(),
            active_sessions: vec![],
            dag_cache: HashMap::new(),
            retry: RetryPolicy::default(),
            quota: TenantQuotaTracker::new(),
            kill_switch: SchedulerKillSwitch::new(),
        }
    }

    pub async fn run(&mut self) {
        let mut tick = interval(Duration::from_millis(500));
        info!("Scheduler loop started");
        loop {
            tick.tick().await;
            if let Err(e) = self.tick().await {
                warn!("scheduler tick error: {e}");
            }
        }
    }

    async fn tick(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // 1. Load active sessions (use projection read)
        if self.active_sessions.is_empty() {
            // Start with a known session — in production, this would be discovered
            self.active_sessions.push(Uuid::nil());
        }

        for sid in &self.active_sessions.clone() {
            let _session = self.client.get_session(GetSessionRequest {
                session_id: sid.to_string(),
            }).await;

            let tasks_resp = self.client.list_tasks(ListTasksRequest {
                session_id: sid.to_string(),
                state_filter: String::new(),
            }).await;

            if let Ok(resp) = tasks_resp {
                let inner = resp.into_inner();
                let states: HashMap<Uuid, TaskState> = inner.tasks.iter()
                    .filter_map(|t| {
                        let tid = Uuid::parse_str(&t.task_id).ok()?;
                        let state = TaskState::from_str(&t.state)?;
                        Some((tid, state))
                    }).collect();

                if let Some(dag) = self.dag_cache.get(sid) {
                    let ready = dag.ready_tasks(&states);
                    for tid in ready {
                        if !self.kill_switch.can_schedule("") {
                            continue;
                        }
                        self.lease_task(*sid, tid).await;
                    }
                }
            }

            // Check for expired leases and time them out
            let running: Vec<_> = if let Ok(resp) = self.client.list_tasks(ListTasksRequest {
                session_id: sid.to_string(),
                state_filter: "Running".into(),
            }).await {
                resp.into_inner().tasks
            } else { continue };

            for t in running {
                let tid = Uuid::parse_str(&t.task_id)?;
                // If fencing_token is stale due to timeout, mark it
                if t.current_fencing_token > 0 && t.state == "Running" {
                    // In production, check lease expiry time
                    // For now, skip automatic timeouts
                }
            }
        }
        Ok(())
    }

    async fn lease_task(&mut self, session_id: Uuid, task_id: Uuid) {
        let task = match self.client.get_task(
            openforce_proto::swarmos::v1::GetTaskRequest {
                session_id: session_id.to_string(),
                task_id: task_id.to_string(),
            }
        ).await {
            Ok(r) => r.into_inner(),
            Err(_) => return,
        };

        if task.state != "Ready" { return; }

        let producer = ProducerIdentity {
            component: "scheduler".into(),
            instance_id: self.instance_id.clone(),
            region: "local".into(),
        };

        let cmd = ProtoCommand {
            command_id: Uuid::now_v7().to_string(),
            command_type: "LeaseTask".into(),
            tenant_id: Uuid::nil().to_string(),
            session_id: session_id.to_string(),
            task_id: task_id.to_string(),
            expected_version: 0,
            requested_by: Some(openforce_proto::swarmos::v1::ProducerIdentity {
                component: producer.component.clone(),
                instance_id: producer.instance_id.clone(),
                region: producer.region.clone(),
            }),
            requested_at: None,
            payload: vec![],
        };

        let result = self.client.execute_command(ExecuteCommandRequest {
            command: Some(cmd),
        }).await;

        match result {
            Ok(resp) => {
                let r = resp.into_inner();
                let result: serde_json::Value = serde_json::from_slice(&r.result).unwrap_or_default();
                info!("leased task {task_id}: lease={}, fencing={}",
                    result.get("lease_id").and_then(|v| v.as_str()).unwrap_or("?"),
                    result.get("fencing_token").and_then(|v| v.as_u64()).unwrap_or(0));
            }
            Err(e) => warn!("lease task {task_id} failed: {e}"),
        }
    }
}

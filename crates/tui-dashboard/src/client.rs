use openforce_proto::swarmos::v1::{
    approval_service_client::ApprovalServiceClient,
    project_tool_service_client::ProjectToolServiceClient, scheduler_client::SchedulerClient,
    session_store_client::SessionStoreClient, Command as ProtoCommand, GetSessionRequest,
    ListTasksRequest, ProducerIdentity,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tonic::transport::Channel;
use uuid::Uuid;

pub struct GrpcClients {
    pub session_store: SessionStoreClient<Channel>,
    pub scheduler: SchedulerClient<Channel>,
    #[allow(dead_code)]
    pub project_tools: ProjectToolServiceClient<Channel>,
    pub approval: ApprovalServiceClient<Channel>,
}

impl GrpcClients {
    pub async fn connect(
        ss_addr: &str,
        sched_addr: &str,
        pt_addr: &str,
    ) -> Result<Self, tonic::transport::Error> {
        let ss = SessionStoreClient::connect(format!("http://{ss_addr}")).await?;
        let sched = SchedulerClient::connect(format!("http://{sched_addr}")).await?;
        let pt = ProjectToolServiceClient::connect(format!("http://{pt_addr}")).await?;
        let ap = ApprovalServiceClient::connect(format!("http://{pt_addr}")).await?;
        Ok(Self {
            session_store: ss,
            scheduler: sched,
            project_tools: pt,
            approval: ap,
        })
    }
}

pub async fn fetch_session_info(
    client: &mut SessionStoreClient<Channel>,
    session_id: &Uuid,
) -> Result<serde_json::Value, String> {
    let resp = client
        .get_session(GetSessionRequest {
            session_id: session_id.to_string(),
        })
        .await
        .map_err(|e| e.to_string())?;
    let inner = resp.into_inner();
    Ok(serde_json::json!({
        "session_id": inner.session_id,
        "goal": inner.goal,
        "state": inner.state,
        "plan_version": inner.current_plan_version,
        "plan_epoch": inner.current_plan_epoch,
        "session_version": inner.session_version,
    }))
}

pub async fn fetch_tasks(
    client: &mut SessionStoreClient<Channel>,
    session_id: &Uuid,
) -> Result<Vec<serde_json::Value>, String> {
    let resp = client
        .list_tasks(ListTasksRequest {
            session_id: session_id.to_string(),
            state_filter: String::new(),
        })
        .await
        .map_err(|e| e.to_string())?;

    let tasks: Vec<serde_json::Value> = resp
        .into_inner()
        .tasks
        .iter()
        .map(|t| {
            serde_json::json!({
                "task_id": t.task_id.clone(),
                "task_type": t.task_type.clone(),
                "state": t.state.clone(),
                "attempt": t.task_attempt,
                "fencing": t.current_fencing_token,
                "lease_id": t.current_lease_id.clone(),
            })
        })
        .collect();
    Ok(tasks)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: Uuid,
    pub goal: String,
    pub state: String,
    pub current_phase: String,
    pub created_at: String,
}

/// List sessions from local .openforce/sessions/ directory
pub fn list_sessions(workspace: &PathBuf) -> Vec<SessionSummary> {
    let dir = workspace.join(".openforce").join("sessions");
    if !dir.exists() {
        return vec![];
    }
    let mut out = vec![];
    for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        if let Ok(json) = std::fs::read_to_string(entry.path()) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json) {
                if let (Some(sid), Some(goal)) = (v["session_id"].as_str(), v["goal"].as_str()) {
                    if let Ok(id) = Uuid::parse_str(sid) {
                        out.push(SessionSummary {
                            session_id: id,
                            goal: goal.to_string(),
                            state: v["state"].as_str().unwrap_or("Active").to_string(),
                            current_phase: v["current_phase"]
                                .as_str()
                                .unwrap_or("Understand")
                                .to_string(),
                            created_at: v["created_at"].as_str().unwrap_or("").to_string(),
                        });
                    }
                }
            }
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.created_at.clone()));
    out
}

pub fn producer_identity(component: &str) -> ProducerIdentity {
    ProducerIdentity {
        component: component.to_string(),
        instance_id: format!(
            "tui-{}",
            uuid::Uuid::now_v7()
                .to_string()
                .chars()
                .take(8)
                .collect::<String>()
        ),
        region: "local".to_string(),
    }
}

pub fn build_command(
    command_type: &str,
    session_id: &Uuid,
    task_id: Option<&str>,
    payload: Vec<u8>,
) -> ProtoCommand {
    ProtoCommand {
        command_id: uuid::Uuid::now_v7().to_string(),
        command_type: command_type.to_string(),
        tenant_id: "default".to_string(),
        session_id: session_id.to_string(),
        task_id: task_id.unwrap_or("").to_string(),
        expected_version: 0,
        requested_by: Some(producer_identity("tui-dashboard")),
        requested_at: Some(prost_types::Timestamp {
            seconds: chrono::Utc::now().timestamp(),
            nanos: 0,
        }),
        payload,
    }
}

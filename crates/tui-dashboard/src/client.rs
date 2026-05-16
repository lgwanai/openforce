use tonic::transport::Channel;
use uuid::Uuid;
use openforce_proto::swarmos::v1::{
    session_store_client::SessionStoreClient,
    scheduler_client::SchedulerClient,
    project_tool_service_client::ProjectToolServiceClient,
    approval_service_client::ApprovalServiceClient,
    GetSessionRequest, ListTasksRequest, GetTaskRequest,
    CompilePlanRequest, CancelTaskRequest, LeaseTaskRequest,
    Command as ProtoCommand, ProducerIdentity,
};

pub struct GrpcClients {
    pub session_store: SessionStoreClient<Channel>,
    pub scheduler: SchedulerClient<Channel>,
    pub project_tools: ProjectToolServiceClient<Channel>,
    pub approval: ApprovalServiceClient<Channel>,
}

impl GrpcClients {
    pub async fn connect(
        ss_addr: &str, sched_addr: &str, pt_addr: &str,
    ) -> Result<Self, tonic::transport::Error> {
        let ss = SessionStoreClient::connect(format!("http://{ss_addr}")).await?;
        let sched = SchedulerClient::connect(format!("http://{sched_addr}")).await?;
        let pt = ProjectToolServiceClient::connect(format!("http://{pt_addr}")).await?;
        let ap = ApprovalServiceClient::connect(format!("http://{pt_addr}")).await?;
        Ok(Self { session_store: ss, scheduler: sched, project_tools: pt, approval: ap })
    }
}

/// Fetch session info for display
pub async fn fetch_session_info(
    client: &mut SessionStoreClient<Channel>, session_id: &str,
) -> Result<serde_json::Value, String> {
    let resp = client.get_session(GetSessionRequest {
        session_id: session_id.to_string(),
    }).await.map_err(|e| e.to_string())?;
    let inner = resp.into_inner();
    Ok(serde_json::json!({
        "session_id": inner.session_id,
        "goal": inner.goal,
        "state": inner.state,
        "plan_version": inner.current_plan_version,
        "session_version": inner.session_version,
    }))
}

/// Fetch all tasks for a session
pub async fn fetch_tasks(
    client: &mut SessionStoreClient<Channel>, session_id: &str, state_filter: &str,
) -> Result<Vec<serde_json::Value>, String> {
    let resp = client.list_tasks(ListTasksRequest {
        session_id: session_id.to_string(),
        state_filter: state_filter.to_string(),
    }).await.map_err(|e| e.to_string())?;

    let tasks: Vec<serde_json::Value> = resp.into_inner().tasks.iter().map(|t| {
        serde_json::json!({
            "task_id": &t.task_id[..8.min(t.task_id.len())],
            "task_type": t.task_type,
            "state": t.state,
            "attempt": t.task_attempt,
            "fencing": t.current_fencing_token,
            "lease_id": &t.current_lease_id[..8.min(t.current_lease_id.len())],
        })
    }).collect();
    Ok(tasks)
}

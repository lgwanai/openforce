use tonic::{Request, Response, Status};
use uuid::Uuid;
use openforce_proto::swarmos::v1::{
    scheduler_server::Scheduler as SchedulerTrait,
    session_store_client::SessionStoreClient,
    CompilePlanRequest, CompilePlanResponse, LeaseTaskRequest, LeaseTaskResponse,
    RenewLeaseRequest, RenewLeaseResponse,
    SubmitArtifactRequest, SubmitArtifactResponse,
    SubmitPatchRequest, SubmitPatchResponse,
    SubmitFindingRequest, SubmitFindingResponse,
    MarkTaskSucceededRequest, MarkTaskSucceededResponse,
    MarkTaskTimedOutRequest, MarkTaskTimedOutResponse,
    ReplanSessionRequest, ReplanSessionResponse,
    CancelTaskRequest, CancelTaskResponse,
    SendHeartbeatRequest, SendHeartbeatResponse,
    ExecuteCommandRequest, ExecuteCommandResponse, Command as ProtoCommand,
};

#[derive(Clone)]
pub struct SchedulerService {
    pub session_store_addr: String,
    pub instance_id: String,
}

impl SchedulerService {
    async fn delegate_command(&self, cmd: ProtoCommand) -> Result<ExecuteCommandResponse, Status> {
        let mut client = SessionStoreClient::connect(format!("http://{}", self.session_store_addr))
            .await.map_err(|e| Status::unavailable(format!("session store: {e}")))?;
        client.execute_command(ExecuteCommandRequest { command: Some(cmd) })
            .await.map(|r| r.into_inner())
            .map_err(|e| Status::internal(format!("execute: {e}")))
    }
}

#[tonic::async_trait]
impl SchedulerTrait for SchedulerService {
    async fn compile_plan(&self, r: Request<CompilePlanRequest>) -> Result<Response<CompilePlanResponse>, Status> {
        let cmd = r.into_inner().command.ok_or(Status::invalid_argument("command required"))?;
        let resp = self.delegate_command(cmd).await?;
        let result: serde_json::Value = serde_json::from_slice(&resp.result).unwrap_or_default();
        Ok(Response::new(CompilePlanResponse {
            plan_version: result.get("plan_version").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            plan_epoch: result.get("plan_epoch").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
        }))
    }

    async fn lease_task(&self, r: Request<LeaseTaskRequest>) -> Result<Response<LeaseTaskResponse>, Status> {
        let cmd = r.into_inner().command.ok_or(Status::invalid_argument("command required"))?;
        let resp = self.delegate_command(cmd).await?;
        let r: serde_json::Value = serde_json::from_slice(&resp.result).unwrap_or_default();
        Ok(Response::new(LeaseTaskResponse {
            lease_id: r.get("lease_id").and_then(|v| v.as_str()).unwrap_or("").into(),
            fencing_token: r.get("fencing_token").and_then(|v| v.as_u64()).unwrap_or(0),
            worker_spec_id: r.get("worker_spec_id").and_then(|v| v.as_str()).unwrap_or("").into(),
            task_attempt: r.get("task_attempt").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            lease_expire_at: None,
        }))
    }

    async fn renew_lease(&self, r: Request<RenewLeaseRequest>) -> Result<Response<RenewLeaseResponse>, Status> {
        let cmd = r.into_inner().command.ok_or(Status::invalid_argument("command required"))?;
        let _ = self.delegate_command(cmd).await?;
        Ok(Response::new(RenewLeaseResponse { new_expire_at: None }))
    }

    async fn submit_artifact(&self, r: Request<SubmitArtifactRequest>) -> Result<Response<SubmitArtifactResponse>, Status> {
        let cmd = r.into_inner().command.ok_or(Status::invalid_argument("command required"))?;
        let resp = self.delegate_command(cmd).await?;
        let r: serde_json::Value = serde_json::from_slice(&resp.result).unwrap_or_default();
        Ok(Response::new(SubmitArtifactResponse {
            artifact_id: r.get("artifact_id").and_then(|v| v.as_str()).unwrap_or("").into(),
            formal_uri: r.get("formal_uri").and_then(|v| v.as_str()).unwrap_or("").into(),
        }))
    }

    async fn submit_patch(&self, r: Request<SubmitPatchRequest>) -> Result<Response<SubmitPatchResponse>, Status> {
        let cmd = r.into_inner().command.ok_or(Status::invalid_argument("command required"))?;
        let resp = self.delegate_command(cmd).await?;
        let r: serde_json::Value = serde_json::from_slice(&resp.result).unwrap_or_default();
        Ok(Response::new(SubmitPatchResponse {
            merge_commit_id: r.get("merge_commit_id").and_then(|v| v.as_str()).unwrap_or("").into(),
            new_snapshot_id: r.get("new_snapshot_id").and_then(|v| v.as_str()).unwrap_or("").into(),
        }))
    }

    async fn submit_finding(&self, r: Request<SubmitFindingRequest>) -> Result<Response<SubmitFindingResponse>, Status> {
        let cmd = r.into_inner().command.ok_or(Status::invalid_argument("command required"))?;
        let resp = self.delegate_command(cmd).await?;
        let r: serde_json::Value = serde_json::from_slice(&resp.result).unwrap_or_default();
        Ok(Response::new(SubmitFindingResponse {
            finding_id: r.get("finding_id").and_then(|v| v.as_str()).unwrap_or("").into(),
        }))
    }

    async fn mark_task_succeeded(&self, r: Request<MarkTaskSucceededRequest>) -> Result<Response<MarkTaskSucceededResponse>, Status> {
        let cmd = r.into_inner().command.ok_or(Status::invalid_argument("command required"))?;
        self.delegate_command(cmd).await?;
        Ok(Response::new(MarkTaskSucceededResponse {}))
    }

    async fn mark_task_timed_out(&self, r: Request<MarkTaskTimedOutRequest>) -> Result<Response<MarkTaskTimedOutResponse>, Status> {
        let cmd = r.into_inner().command.ok_or(Status::invalid_argument("command required"))?;
        self.delegate_command(cmd).await?;
        Ok(Response::new(MarkTaskTimedOutResponse {}))
    }

    async fn replan_session(&self, r: Request<ReplanSessionRequest>) -> Result<Response<ReplanSessionResponse>, Status> {
        let cmd = r.into_inner().command.ok_or(Status::invalid_argument("command required"))?;
        let resp = self.delegate_command(cmd).await?;
        let r: serde_json::Value = serde_json::from_slice(&resp.result).unwrap_or_default();
        Ok(Response::new(ReplanSessionResponse {
            new_plan_version: r.get("new_plan_version").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
            new_plan_epoch: r.get("new_plan_epoch").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
        }))
    }

    async fn cancel_task(&self, r: Request<CancelTaskRequest>) -> Result<Response<CancelTaskResponse>, Status> {
        let cmd = r.into_inner().command.ok_or(Status::invalid_argument("command required"))?;
        self.delegate_command(cmd).await?;
        Ok(Response::new(CancelTaskResponse {}))
    }

    async fn send_heartbeat(&self, r: Request<SendHeartbeatRequest>) -> Result<Response<SendHeartbeatResponse>, Status> {
        let _req = r.into_inner();
        Ok(Response::new(SendHeartbeatResponse { lease_expire_at: None, lease_valid: true }))
    }
}

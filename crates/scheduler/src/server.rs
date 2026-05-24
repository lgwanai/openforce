use std::sync::Arc;
use tokio::sync::Mutex;
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
use crate::capability_token::CapabilityTokenIssuer;

#[derive(Clone)]
pub struct SchedulerService {
    pub session_store_addr: String,
    pub instance_id: String,
    pub token_issuer: Option<Arc<CapabilityTokenIssuer>>,
    /// Reused gRPC connection — established once, shared across requests.
    client: Arc<Mutex<Option<SessionStoreClient<tonic::transport::Channel>>>>,
}

impl SchedulerService {
    pub fn new(session_store_addr: String, instance_id: String, token_issuer: Option<Arc<CapabilityTokenIssuer>>) -> Self {
        Self {
            session_store_addr,
            instance_id,
            token_issuer,
            client: Arc::new(Mutex::new(None)),
        }
    }

    /// Get or lazily create a persistent gRPC client connection.
    async fn get_client(&self) -> Result<SessionStoreClient<tonic::transport::Channel>, Status> {
        let mut guard = self.client.lock().await;
        if guard.is_none() {
            let addr = format!("http://{}", self.session_store_addr);
            let client = SessionStoreClient::connect(addr).await
                .map_err(|e| Status::unavailable(format!("session store connect: {e}")))?;
            *guard = Some(client);
        }
        // Clone the client — tonic Channel is multiplexed and cheap to clone
        Ok(guard.as_ref().unwrap().clone())
    }

    async fn delegate_command(&self, cmd: ProtoCommand) -> Result<ExecuteCommandResponse, Status> {
        let mut client = self.get_client().await?;
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
        let tenant_id = cmd.tenant_id.parse::<Uuid>().unwrap_or(Uuid::nil());
        let session_id = cmd.session_id.parse::<Uuid>().unwrap_or(Uuid::nil());
        let task_id = cmd.task_id.parse::<Uuid>().unwrap_or(Uuid::nil());
        let resp = self.delegate_command(cmd).await?;
        let r: serde_json::Value = serde_json::from_slice(&resp.result).unwrap_or_default();
        let lease_id: Uuid = r.get("lease_id").and_then(|v| v.as_str()).and_then(|s| Uuid::parse_str(s).ok()).unwrap_or(Uuid::nil());
        let fencing_token = r.get("fencing_token").and_then(|v| v.as_u64()).unwrap_or(0);
        let task_attempt = r.get("task_attempt").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let plan_epoch = r.get("plan_epoch").and_then(|v| v.as_i64()).unwrap_or(0) as i32;

        // Issue capability token if issuer configured
        let capability_token = match &self.token_issuer {
            Some(issuer) => issuer.issue(
                tenant_id, session_id, task_id, task_attempt,
                lease_id, fencing_token, plan_epoch,
                vec![
                    openforce_domain::token::TokenScope::artifact_submit(),
                    openforce_domain::token::TokenScope::patch_submit(),
                    openforce_domain::token::TokenScope::finding_submit(),
                    openforce_domain::token::TokenScope::effect_request(),
                    openforce_domain::token::TokenScope::heartbeat_write(),
                ],
            ).unwrap_or_default(),
            None => String::new(),
        };

        Ok(Response::new(LeaseTaskResponse {
            lease_id: lease_id.to_string(),
            fencing_token,
            worker_spec_id: r.get("worker_spec_id").and_then(|v| v.as_str()).unwrap_or("").into(),
            task_attempt,
            lease_expire_at: None,
            capability_token,
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
        let req = r.into_inner();
        let tenant_id = req.tenant_id;
        let session_id = req.session_id;
        let task_id = req.task_id;
        let fencing_token = req.fencing_token;

        if tenant_id.is_empty() || session_id.is_empty() || task_id.is_empty() {
            return Err(Status::invalid_argument("tenant_id, session_id, and task_id are required"));
        }
        if fencing_token == 0 {
            return Err(Status::invalid_argument("fencing_token is required for heartbeat"));
        }

        // Delegate heartbeat to session-store which verifies the lease is active
        let cmd = ProtoCommand {
            command_id: Uuid::now_v7().to_string(),
            command_type: "RenewLease".into(),
            tenant_id,
            session_id,
            task_id,
            expected_version: 0,
            payload: serde_json::to_vec(&serde_json::json!({"fencing_token": fencing_token})).unwrap_or_default(),
            ..Default::default()
        };

        match self.delegate_command(cmd).await {
            Ok(_) => Ok(Response::new(SendHeartbeatResponse {
                lease_valid: true,
                lease_expire_at: None,
            })),
            Err(e) => {
                tracing::warn!("heartbeat rejected: {e}");
                Ok(Response::new(SendHeartbeatResponse {
                    lease_valid: false,
                    lease_expire_at: None,
                }))
            }
        }
    }
}

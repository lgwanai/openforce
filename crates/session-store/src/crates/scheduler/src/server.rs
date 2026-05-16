use tonic::{Request, Response, Status};
use uuid::Uuid;

use openforce_proto::swarmos::v1::{
    scheduler_server::Scheduler as SchedulerTrait,
    CompilePlanRequest, CompilePlanResponse,
    LeaseTaskRequest, LeaseTaskResponse,
    RenewLeaseRequest, RenewLeaseResponse,
    SubmitArtifactRequest, SubmitArtifactResponse,
    SubmitPatchRequest, SubmitPatchResponse,
    SubmitFindingRequest, SubmitFindingResponse,
    MarkTaskSucceededRequest, MarkTaskSucceededResponse,
    MarkTaskTimedOutRequest, MarkTaskTimedOutResponse,
    ReplanSessionRequest, ReplanSessionResponse,
    CancelTaskRequest, CancelTaskResponse,
    SendHeartbeatRequest, SendHeartbeatResponse,
};

#[derive(Clone)]
pub struct SchedulerService {
    pub session_store_addr: String,
    pub instance_id: String,
}

#[tonic::async_trait]
impl SchedulerTrait for SchedulerService {
    async fn compile_plan(&self, _r: Request<CompilePlanRequest>) -> Result<Response<CompilePlanResponse>, Status> {
        Err(Status::unimplemented("compile_plan delegated to session-store"))
    }
    async fn lease_task(&self, _r: Request<LeaseTaskRequest>) -> Result<Response<LeaseTaskResponse>, Status> {
        Err(Status::unimplemented("lease_task delegated to session-store"))
    }
    async fn renew_lease(&self, _r: Request<RenewLeaseRequest>) -> Result<Response<RenewLeaseResponse>, Status> {
        Err(Status::unimplemented("renew_lease not implemented"))
    }
    async fn submit_artifact(&self, _r: Request<SubmitArtifactRequest>) -> Result<Response<SubmitArtifactResponse>, Status> {
        Err(Status::unimplemented("submit_artifact not implemented"))
    }
    async fn submit_patch(&self, _r: Request<SubmitPatchRequest>) -> Result<Response<SubmitPatchResponse>, Status> {
        Err(Status::unimplemented("submit_patch not implemented"))
    }
    async fn submit_finding(&self, _r: Request<SubmitFindingRequest>) -> Result<Response<SubmitFindingResponse>, Status> {
        Err(Status::unimplemented("submit_finding not implemented"))
    }
    async fn mark_task_succeeded(&self, _r: Request<MarkTaskSucceededRequest>) -> Result<Response<MarkTaskSucceededResponse>, Status> {
        Err(Status::unimplemented("mark_task_succeeded not implemented"))
    }
    async fn mark_task_timed_out(&self, _r: Request<MarkTaskTimedOutRequest>) -> Result<Response<MarkTaskTimedOutResponse>, Status> {
        Err(Status::unimplemented("mark_task_timed_out not implemented"))
    }
    async fn replan_session(&self, _r: Request<ReplanSessionRequest>) -> Result<Response<ReplanSessionResponse>, Status> {
        Err(Status::unimplemented("replan_session not implemented"))
    }
    async fn cancel_task(&self, _r: Request<CancelTaskRequest>) -> Result<Response<CancelTaskResponse>, Status> {
        Err(Status::unimplemented("cancel_task not implemented"))
    }
    async fn send_heartbeat(&self, _r: Request<SendHeartbeatRequest>) -> Result<Response<SendHeartbeatResponse>, Status> {
        Err(Status::unimplemented("send_heartbeat not implemented"))
    }
}

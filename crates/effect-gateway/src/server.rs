use tonic::{Request, Response, Status};
use uuid::Uuid;
use openforce_proto::swarmos::v1::{
    effect_gateway_server::EffectGateway as EffectGatewayTrait,
    RequestEffectRequest, RequestEffectResponse,
    GetEffectRequest, GetEffectResponse,
    ApproveEffectRequest, ApproveEffectResponse,
};
use crate::store::EffectStore;

#[derive(Clone)]
pub struct EffectGatewayService {
    pub store: std::sync::Arc<EffectStore>,
}

#[tonic::async_trait]
impl EffectGatewayTrait for EffectGatewayService {
    async fn request_effect(&self, r: Request<RequestEffectRequest>) -> Result<Response<RequestEffectResponse>, Status> {
        let req = r.into_inner();
        let cmd = req.command.ok_or(Status::invalid_argument("command required"))?;
        let sid = Uuid::parse_str(&cmd.session_id).map_err(|_| Status::invalid_argument("invalid session_id"))?;
        let tid = Uuid::parse_str(&cmd.tenant_id).map_err(|_| Status::invalid_argument("invalid tenant_id"))?;
        let task_id = if cmd.task_id.is_empty() { None } else { Some(Uuid::parse_str(&cmd.task_id).map_err(|_| Status::invalid_argument("invalid task_id"))?) };
        let (eid, status) = self.store.request_effect(
            Uuid::now_v7(), sid, task_id, tid,
            &req.effect_type, &req.target, &req.idempotency_key, &req.payload_ref,
        ).await.map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(RequestEffectResponse { effect_id: eid.to_string(), status: status.as_str().into() }))
    }

    async fn get_effect(&self, r: Request<GetEffectRequest>) -> Result<Response<GetEffectResponse>, Status> {
        let eid = Uuid::parse_str(&r.into_inner().effect_id).map_err(|_| Status::invalid_argument("invalid effect_id"))?;
        let (status, key, _) = self.store.get_effect(eid).await.map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(GetEffectResponse { effect_id: eid.to_string(), status, idempotency_key: key }))
    }

    async fn approve_effect(&self, r: Request<ApproveEffectRequest>) -> Result<Response<ApproveEffectResponse>, Status> {
        let req = r.into_inner();
        let eid = Uuid::parse_str(&req.effect_id).map_err(|_| Status::invalid_argument("invalid effect_id"))?;
        let status = self.store.approve_effect(eid, &req.approved_by).await.map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ApproveEffectResponse { effect_id: eid.to_string(), status: status.as_str().into() }))
    }
}

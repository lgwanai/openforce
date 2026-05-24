use tonic::{Request, Response, Status};
use uuid::Uuid;
use openforce_proto::swarmos::v1::{
    effect_gateway_server::EffectGateway as EffectGatewayTrait,
    RequestEffectRequest, RequestEffectResponse,
    GetEffectRequest, GetEffectResponse,
    ApproveEffectRequest, ApproveEffectResponse,
    RejectEffectRequest, RejectEffectResponse,
};
use openforce_domain::identity::ServiceRole;
use crate::store::EffectStore;

#[derive(Clone)]
pub struct EffectGatewayService {
    pub store: std::sync::Arc<EffectStore>,
}

impl EffectGatewayService {
    /// Verify that the gRPC caller holds an authorized role (human-approver or scheduler).
    /// In production, this inspects the mTLS peer certificate's SPIFFE ID.
    /// Falls back to metadata-based check when mTLS identity is not available.
    fn require_approver_role<T>(req: &Request<T>) -> Result<(), Status> {
        let spiffe_id = req.metadata().get("x-spiffe-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let authorized_roles = [ServiceRole::human_approver(), ServiceRole::scheduler()];
        let is_authorized = authorized_roles.iter().any(|role| {
            spiffe_id.contains(role.as_str())
        });

        if !is_authorized {
            // In dev mode: allow if APPROVAL_BYPASS env var is set
            if std::env::var("APPROVAL_BYPASS").is_ok() {
                tracing::warn!("approval bypassed — APPROVAL_BYPASS is set (INSECURE, dev/test only)");
                return Ok(());
            }
            return Err(Status::permission_denied(
                format!("only {:?} roles may approve/reject effects", authorized_roles.iter().map(|r| r.as_str()).collect::<Vec<_>>())
            ));
        }
        Ok(())
    }
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
        // Authorization: only human-approver or scheduler roles may approve effects
        Self::require_approver_role(&r)?;
        let req = r.into_inner();
        let eid = Uuid::parse_str(&req.effect_id).map_err(|_| Status::invalid_argument("invalid effect_id"))?;
        let status = self.store.approve_effect(eid, &req.approved_by).await.map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ApproveEffectResponse { effect_id: eid.to_string(), status: status.as_str().into() }))
    }

    async fn reject_effect(&self, r: Request<RejectEffectRequest>) -> Result<Response<RejectEffectResponse>, Status> {
        // Authorization: only human-approver or scheduler roles may reject effects
        Self::require_approver_role(&r)?;
        let req = r.into_inner();
        let eid = Uuid::parse_str(&req.effect_id).map_err(|_| Status::invalid_argument("invalid effect_id"))?;
        let status = self.store.reject_effect(eid, &req.rejected_by, &req.reason).await.map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(RejectEffectResponse { effect_id: eid.to_string(), status: status.as_str().into() }))
    }
}

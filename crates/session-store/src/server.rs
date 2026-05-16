use chrono::Datelike; use chrono::Timelike;
use sqlx::PgPool; use tonic::{Request, Response, Status}; use uuid::Uuid;
use openforce_proto::swarmos::v1::{
    session_store_server::SessionStore as SessionStoreTrait,
    AppendEventsRequest, AppendEventsResponse, ExecuteCommandRequest, ExecuteCommandResponse,
    GetSessionRequest, GetSessionResponse, GetTaskRequest, GetTaskResponse,
    ListTasksRequest, ListTasksResponse, ReadEventLogRequest, RebuildProjectionRequest,
    RebuildProjectionResponse,
};
use openforce_domain::event::ProducerIdentity;
use openforce_domain::command::{Command as DomainCommand, CommandType};
use crate::store::EventStore; use crate::command_handler::CmdHandler;
use crate::repo::ProjectionRepo; use crate::projection::ProjectionBuilder;

#[derive(Clone)] pub struct SessionStoreService { pub pool: PgPool, pub instance_id: String }

fn pu(s: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(s).map_err(|_| Status::invalid_argument("invalid UUID"))
}
fn prod(svc: &SessionStoreService) -> ProducerIdentity {
    ProducerIdentity { component: "session-store".into(), instance_id: svc.instance_id.clone(), region: "local".into() }
}
fn de(e: openforce_domain::error::DomainError) -> Status {
    use openforce_domain::error::DomainError;
    match &e {
        DomainError::VersionConflict{..} => Status::aborted(e.to_string()),
        DomainError::CommandReplayed{..} => Status::already_exists(e.to_string()),
        DomainError::InvalidTransition{..} => Status::failed_precondition(e.to_string()),
        DomainError::LeaseExpired => Status::deadline_exceeded(e.to_string()),
        DomainError::FencingTokenStale{..} => Status::failed_precondition(e.to_string()),
        DomainError::TaskNotReady => Status::failed_precondition(e.to_string()),
        DomainError::SessionNotFound{..} | DomainError::TaskNotFound{..} | DomainError::TenantNotFound{..} => Status::not_found(e.to_string()),
        DomainError::QuotaExceeded{..} => Status::resource_exhausted(e.to_string()),
        _ => Status::internal(e.to_string()),
    }
}
fn ts(dt: chrono::DateTime<chrono::Utc>) -> Option<prost_types::Timestamp> {
    prost_types::Timestamp::date_time(dt.year() as i64, dt.month() as u8, dt.day() as u8,
        dt.hour() as u8, dt.minute() as u8, dt.second() as u8).ok()
}

#[tonic::async_trait]
impl SessionStoreTrait for SessionStoreService {
    async fn append_events(&self, r: Request<AppendEventsRequest>) -> Result<Response<AppendEventsResponse>, Status> {
        let req = r.into_inner(); let sid = pu(&req.session_id)?; let tid = pu(&req.tenant_id)?;
        let events: Vec<openforce_domain::event::EventEnvelope> = req.events.into_iter().map(|e| {
            let p = e.producer.clone();
            openforce_domain::event::EventEnvelope {
                event_id: pu(&e.event_id).unwrap_or_else(|_| Uuid::now_v7()),
                event_type: e.event_type, session_id: sid, tenant_id: tid,
                plan_version: e.plan_version, plan_epoch: e.plan_epoch,
                task_id: if e.task_id.is_empty() { None } else { pu(&e.task_id).ok() },
                task_attempt: e.task_attempt,
                producer: ProducerIdentity {
                    component: p.as_ref().map_or(String::new(), |x| x.component.clone()),
                    instance_id: p.as_ref().map_or(String::new(), |x| x.instance_id.clone()),
                    region: p.as_ref().map_or(String::new(), |x| x.region.clone()),
                },
                causation_id: pu(&e.causation_id).unwrap_or_else(|_| Uuid::now_v7()),
                correlation_id: pu(&e.correlation_id).unwrap_or_else(|_| Uuid::now_v7()),
                occurred_at: chrono::Utc::now(), session_version: 0,
                payload: serde_json::from_slice(&e.payload)
                    .unwrap_or(openforce_domain::event::EventPayload::TaskReadied(
                        openforce_domain::event::TaskReadiedPayload{})),
            }
        }).collect();
        let nv = EventStore::append_events(&self.pool, sid, req.expected_version, &events).await.map_err(de)?;
        Ok(Response::new(AppendEventsResponse { new_session_version: nv }))
    }

    async fn execute_command(&self, r: Request<ExecuteCommandRequest>) -> Result<Response<ExecuteCommandResponse>, Status> {
        let req = r.into_inner();
        let c = req.command.ok_or_else(|| Status::invalid_argument("command required"))?;
        let ct = CommandType::from_str(&c.command_type)
            .ok_or_else(|| Status::invalid_argument(format!("unknown: {}", c.command_type)))?;
        let dc = DomainCommand {
            command_id: pu(&c.command_id)?, command_type: ct,
            tenant_id: pu(&c.tenant_id)?, session_id: pu(&c.session_id)?,
            task_id: if c.task_id.is_empty() { None } else { pu(&c.task_id).ok() },
            expected_version: c.expected_version, requested_by: prod(self),
            requested_at: chrono::Utc::now(),
            payload: serde_json::from_slice(&c.payload).unwrap_or(serde_json::Value::Null),
        };
        let res = CmdHandler::execute(&self.pool, dc, prod(self)).await.map_err(de)?;
        let rj = serde_json::to_vec(&res.result).map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ExecuteCommandResponse { result: rj, new_session_version: res.new_session_version }))
    }

    async fn get_session(&self, r: Request<GetSessionRequest>) -> Result<Response<GetSessionResponse>, Status> {
        let sid = pu(&r.into_inner().session_id)?;
        let s = ProjectionRepo::get_session(&self.pool, sid).await.map_err(de)?;
        Ok(Response::new(GetSessionResponse {
            session_id: s.session_id.to_string(), tenant_id: s.tenant_id.to_string(),
            goal: s.goal, state: s.state.as_str().into(),
            current_plan_version: s.current_plan_version, current_plan_epoch: s.current_plan_epoch,
            session_version: s.session_version,
            policy_profile: serde_json::to_vec(&s.policy_profile).unwrap_or_default(),
            created_at: ts(s.created_at), updated_at: ts(s.updated_at),
        }))
    }

    async fn get_task(&self, r: Request<GetTaskRequest>) -> Result<Response<GetTaskResponse>, Status> {
        let req = r.into_inner();
        let t = ProjectionRepo::get_task(&self.pool, pu(&req.session_id)?, pu(&req.task_id)?).await.map_err(de)?;
        Ok(Response::new(GetTaskResponse {
            session_id: t.session_id.to_string(), task_id: t.task_id.to_string(),
            task_type: t.task_type, state: t.state.as_str().into(),
            task_attempt: t.task_attempt,
            current_lease_id: t.current_lease_id.map(|id| id.to_string()).unwrap_or_default(),
            current_fencing_token: t.current_fencing_token,
            current_worker_spec_id: t.current_worker_spec_id.map(|id| id.to_string()).unwrap_or_default(),
            plan_epoch: t.plan_epoch, replan_disposition: t.replan_disposition.as_str().into(),
            upstream_task_ids: t.upstream_task_ids.iter().map(|id| id.to_string()).collect(),
            downstream_task_ids: t.downstream_task_ids.iter().map(|id| id.to_string()).collect(),
            created_at: ts(t.created_at), updated_at: ts(t.updated_at),
        }))
    }

    async fn list_tasks(&self, r: Request<ListTasksRequest>) -> Result<Response<ListTasksResponse>, Status> {
        let req = r.into_inner();
        let sf = if req.state_filter.is_empty() { None } else { Some(req.state_filter) };
        let tasks = ProjectionRepo::list_tasks(&self.pool, pu(&req.session_id)?, sf).await.map_err(de)?;
        let pts: Vec<GetTaskResponse> = tasks.into_iter().map(|t| GetTaskResponse {
            session_id: t.session_id.to_string(), task_id: t.task_id.to_string(),
            task_type: t.task_type, state: t.state.as_str().into(),
            task_attempt: t.task_attempt,
            current_lease_id: t.current_lease_id.map(|id| id.to_string()).unwrap_or_default(),
            current_fencing_token: t.current_fencing_token,
            current_worker_spec_id: t.current_worker_spec_id.map(|id| id.to_string()).unwrap_or_default(),
            plan_epoch: t.plan_epoch, replan_disposition: t.replan_disposition.as_str().into(),
            upstream_task_ids: t.upstream_task_ids.iter().map(|id| id.to_string()).collect(),
            downstream_task_ids: t.downstream_task_ids.iter().map(|id| id.to_string()).collect(),
            created_at: ts(t.created_at), updated_at: ts(t.updated_at),
        }).collect();
        Ok(Response::new(ListTasksResponse { tasks: pts }))
    }

    type ReadEventLogStream = tokio_stream::wrappers::ReceiverStream<Result<openforce_proto::swarmos::v1::Event, Status>>;

    async fn read_event_log(&self, r: Request<ReadEventLogRequest>) -> Result<Response<Self::ReadEventLogStream>, Status> {
        let req = r.into_inner();
        let events = EventStore::read_event_log(&self.pool, pu(&req.session_id)?,
            req.from_session_version, req.max_events).await.map_err(de)?;
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        for e in events {
            let _ = tx.send(Ok(openforce_proto::swarmos::v1::Event {
                event_id: e.event_id.to_string(), event_type: e.event_type,
                session_id: e.session_id.to_string(), tenant_id: e.tenant_id.to_string(),
                plan_version: e.plan_version, plan_epoch: e.plan_epoch,
                task_id: e.task_id.map(|id| id.to_string()).unwrap_or_default(),
                task_attempt: e.task_attempt,
                producer: Some(openforce_proto::swarmos::v1::ProducerIdentity {
                    component: e.producer.component, instance_id: e.producer.instance_id,
                    region: e.producer.region,
                }),
                causation_id: e.causation_id.to_string(),
                correlation_id: e.correlation_id.to_string(),
                session_version: e.session_version,
                payload: serde_json::to_vec(&e.payload).unwrap_or_default(),
                occurred_at: None,
            })).await;
        }
        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn rebuild_projection(&self, r: Request<RebuildProjectionRequest>) -> Result<Response<RebuildProjectionResponse>, Status> {
        let sid = pu(&r.into_inner().session_id)?;
        ProjectionBuilder::rebuild(&self.pool, sid).await.map_err(de)?;
        let s = ProjectionRepo::get_session(&self.pool, sid).await.map_err(de)?;
        Ok(Response::new(RebuildProjectionResponse { success: true, rebuilt_to_version: s.session_version }))
    }
}

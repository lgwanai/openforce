use chrono::Duration;
use sqlx::PgPool; use uuid::Uuid;
use openforce_domain::command::{Command, CommandResult, CommandType};
use openforce_domain::error::{DomainError, DomainResult};
use openforce_domain::event::{EventEnvelope, EventPayload, ProducerIdentity};
use openforce_domain::lease; use openforce_domain::task::TaskState;
use crate::projection::ProjectionBuilder; use crate::repo::ProjectionRepo;
use crate::store::EventStore;

pub struct CmdHandler;

impl CmdHandler {
    pub async fn execute(pool: &PgPool, cmd: Command, producer: ProducerIdentity)
        -> DomainResult<CommandResult>
    {
        if let Some(r) = Self::check_dedup(pool, cmd.command_id).await? { return Ok(r); }
        let result = match cmd.command_type {
            CommandType::CompilePlan => Self::compile_plan(pool, &cmd).await?,
            CommandType::MarkTaskReady => Self::mark_task_ready(pool, &cmd).await?,
            CommandType::LeaseTask => Self::lease_task(pool, &cmd, producer).await?,
            CommandType::RenewLease => Self::renew_lease(pool, &cmd).await?,
            CommandType::SubmitArtifact => Self::submit_artifact(pool, &cmd).await?,
            CommandType::SubmitPatch => Self::submit_patch(pool, &cmd).await?,
            CommandType::SubmitFinding => Self::submit_finding(pool, &cmd).await?,
            CommandType::RequestEffect => Self::request_effect(pool, &cmd).await?,
            CommandType::MarkTaskSucceeded => Self::mark_succeeded(pool, &cmd).await?,
            CommandType::MarkTaskTimedOut => Self::mark_timed_out(pool, &cmd).await?,
            CommandType::ReplanSession => Self::replan(pool, &cmd).await?,
            CommandType::CancelTask => Self::cancel(pool, &cmd).await?,
        };
        Self::save_dedup(pool, cmd.command_id, &cmd, &result).await?;
        Ok(result)
    }

    async fn check_dedup(pool: &PgPool, cid: Uuid) -> DomainResult<Option<CommandResult>> {
        let row: Option<(serde_json::Value,)> = sqlx::query_as(
            "SELECT result FROM command_dedup WHERE command_id = $1"
        ).bind(cid).fetch_optional(pool).await
         .map_err(|e| DomainError::ValidationFailed { detail: e.to_string() })?;
        if let Some((json,)) = row {
            let obj = json.as_object().ok_or(DomainError::ValidationFailed {
                detail: "invalid dedup result".into()
            })?;
            return Ok(Some(CommandResult {
                command_id: cid, success: true,
                new_session_version: obj.get("new_session_version")
                    .and_then(|v| v.as_i64()).unwrap_or(0),
                result: obj.get("result").cloned().unwrap_or(serde_json::json!({})),
            }));
        }
        Ok(None)
    }

    async fn save_dedup(pool: &PgPool, cid: Uuid, cmd: &Command, result: &CommandResult)
        -> DomainResult<()>
    {
        let json = serde_json::json!({
            "command_type": cmd.command_type.as_str(),
            "session_id": cmd.session_id.to_string(),
            "new_session_version": result.new_session_version,
            "result": result.result,
        });
        sqlx::query(
            "INSERT INTO command_dedup (command_id, command_type, session_id, result)
             VALUES ($1, $2, $3, $4) ON CONFLICT (command_id) DO NOTHING"
        ).bind(cid).bind(cmd.command_type.as_str()).bind(cmd.session_id).bind(&json)
         .execute(pool).await.map_err(|e| DomainError::ValidationFailed { detail: e.to_string() })?;
        Ok(())
    }

    fn res(cid: Uuid, ver: i64, r: serde_json::Value) -> CommandResult {
        CommandResult { command_id: cid, success: true, new_session_version: ver, result: r }
    }

    async fn compile_plan(pool: &PgPool, cmd: &Command) -> DomainResult<CommandResult> {
        let ver = ProjectionRepo::get_session(pool, cmd.session_id).await
            .map(|s| s.current_plan_version).unwrap_or(0);
        let nv = ver + 1;
        let evt = EventEnvelope::new("PlanCompiled", cmd.session_id, cmd.tenant_id,
            cmd.requested_by.clone(),
            EventPayload::PlanCompiled(openforce_domain::event::PlanCompiledPayload {
                plan_version: nv, plan_epoch: nv, tasks: vec![], dag_edges: vec![],
            }));
        let new_ver = EventStore::append_events(pool, cmd.session_id, cmd.expected_version, &[evt]).await?;
        ProjectionBuilder::rebuild(pool, cmd.session_id).await?;
        Ok(Self::res(cmd.command_id, new_ver, serde_json::json!({"plan_version": nv, "plan_epoch": nv})))
    }

    async fn mark_task_ready(pool: &PgPool, cmd: &Command) -> DomainResult<CommandResult> {
        let tid = cmd.task_id.ok_or(DomainError::ValidationFailed { detail: "task_id required".into() })?;
        let t = ProjectionRepo::get_task(pool, cmd.session_id, tid).await?;
        if t.state != TaskState::Pending { return Err(DomainError::TaskNotReady); }
        let evt = EventEnvelope::new("TaskReadied", cmd.session_id, cmd.tenant_id,
            cmd.requested_by.clone(), EventPayload::TaskReadied(openforce_domain::event::TaskReadiedPayload{}))
            .with_task(tid, t.task_attempt, 0, t.plan_epoch);
        let new_ver = EventStore::append_events(pool, cmd.session_id, cmd.expected_version, &[evt]).await?;
        ProjectionBuilder::rebuild(pool, cmd.session_id).await?;
        Ok(Self::res(cmd.command_id, new_ver, serde_json::json!({"task_id": tid.to_string(), "state": "Ready"})))
    }

    async fn lease_task(pool: &PgPool, cmd: &Command, prod: ProducerIdentity) -> DomainResult<CommandResult> {
        let tid = cmd.task_id.ok_or(DomainError::ValidationFailed { detail: "task_id required".into() })?;
        let t = ProjectionRepo::get_task(pool, cmd.session_id, tid).await?;
        if t.state != TaskState::Ready { return Err(DomainError::TaskNotReady); }
        let na = t.task_attempt + 1; let ft = t.current_fencing_token + 1;
        let l = lease::Lease::new(cmd.session_id, tid, na, ft, Uuid::now_v7(),
            Duration::seconds(lease::DEFAULT_LEASE_TTL_SECS), lease::DEFAULT_HEARTBEAT_INTERVAL_SECS);
        let evt = EventEnvelope::new("TaskLeased", cmd.session_id, cmd.tenant_id, prod,
            EventPayload::TaskLeased(openforce_domain::event::TaskLeasedPayload {
                lease_id: l.lease_id.to_string(), lease_expire_at: l.expire_at.to_rfc3339(),
                renewal_deadline: l.renewal_deadline.map(|d| d.to_rfc3339()).unwrap_or_default(),
                fencing_token: ft, worker_spec_id: l.worker_spec_id.to_string(),
                assigned_node_pool: "default".into(),
            })).with_task(tid, na, 0, t.plan_epoch);
        let new_ver = EventStore::append_events(pool, cmd.session_id, cmd.expected_version, &[evt]).await?;
        ProjectionBuilder::rebuild(pool, cmd.session_id).await?;
        Ok(Self::res(cmd.command_id, new_ver, serde_json::json!({
            "lease_id": l.lease_id.to_string(), "fencing_token": ft,
            "worker_spec_id": l.worker_spec_id.to_string(), "task_attempt": na,
            "lease_expire_at": l.expire_at.to_rfc3339(),
        })))
    }

    async fn renew_lease(pool: &PgPool, cmd: &Command) -> DomainResult<CommandResult> {
        let tid = cmd.task_id.ok_or(DomainError::ValidationFailed { detail: "task_id required".into() })?;
        let t = ProjectionRepo::get_task(pool, cmd.session_id, tid).await?;
        if !t.state.is_runnable() { return Err(DomainError::LeaseExpired); }
        let evt = EventEnvelope::new("HeartbeatReceived", cmd.session_id, cmd.tenant_id,
            cmd.requested_by.clone(),
            EventPayload::HeartbeatReceived(openforce_domain::event::HeartbeatReceivedPayload {
                lease_id: t.current_lease_id.map(|id| id.to_string()).unwrap_or_default(),
                fencing_token: t.current_fencing_token,
            })).with_task(tid, t.task_attempt, 0, t.plan_epoch);
        let new_ver = EventStore::append_events(pool, cmd.session_id, cmd.expected_version, &[evt]).await?;
        ProjectionBuilder::rebuild(pool, cmd.session_id).await?;
        Ok(Self::res(cmd.command_id, new_ver, serde_json::json!({"renewed": true})))
    }

    async fn submit_artifact(pool: &PgPool, cmd: &Command) -> DomainResult<CommandResult> {
        let tid = cmd.task_id.ok_or(DomainError::ValidationFailed { detail: "task_id required".into() })?;
        let t = ProjectionRepo::get_task(pool, cmd.session_id, tid).await?;
        t.verify_submissible()?;
        t.verify_fencing(t.current_fencing_token)?;
        t.verify_fencing(t.current_fencing_token)?;
        t.verify_fencing(t.current_fencing_token)?;
        let evt = EventEnvelope::new("ArtifactSubmitted", cmd.session_id, cmd.tenant_id,
            cmd.requested_by.clone(),
            EventPayload::ArtifactSubmitted(openforce_domain::event::ArtifactSubmittedPayload {
                artifact_id: Uuid::now_v7().to_string(), artifact_type: "generic".into(),
                storage_uri: String::new(), content_sha256: String::new(),
                produced_by_spec: String::new(), base_snapshot_id: String::new(),
                validation_summary: serde_json::json!({}),
            })).with_task(tid, t.task_attempt, 0, t.plan_epoch);
        let new_ver = EventStore::append_events(pool, cmd.session_id, cmd.expected_version, &[evt]).await?;
        ProjectionBuilder::rebuild(pool, cmd.session_id).await?;
        Ok(Self::res(cmd.command_id, new_ver, serde_json::json!({"task_id": tid.to_string()})))
    }

    async fn submit_patch(pool: &PgPool, cmd: &Command) -> DomainResult<CommandResult> {
        let tid = cmd.task_id.ok_or(DomainError::ValidationFailed { detail: "task_id required".into() })?;
        let t = ProjectionRepo::get_task(pool, cmd.session_id, tid).await?;
        t.verify_submissible()?;
        t.verify_fencing(t.current_fencing_token)?;
        t.verify_fencing(t.current_fencing_token)?;
        t.verify_fencing(t.current_fencing_token)?;
        let evt = EventEnvelope::new("PatchSubmitted", cmd.session_id, cmd.tenant_id,
            cmd.requested_by.clone(),
            EventPayload::PatchSubmitted(openforce_domain::event::PatchSubmittedPayload {
                patch_ref: String::new(), patch_sha256: String::new(), target_paths: vec![],
                base_snapshot_id: String::new(), merge_commit_id: None,
            })).with_task(tid, t.task_attempt, 0, t.plan_epoch);
        let new_ver = EventStore::append_events(pool, cmd.session_id, cmd.expected_version, &[evt]).await?;
        ProjectionBuilder::rebuild(pool, cmd.session_id).await?;
        Ok(Self::res(cmd.command_id, new_ver, serde_json::json!({"task_id": tid.to_string()})))
    }

    async fn submit_finding(pool: &PgPool, cmd: &Command) -> DomainResult<CommandResult> {
        let tid = cmd.task_id.ok_or(DomainError::ValidationFailed { detail: "task_id required".into() })?;
        let t = ProjectionRepo::get_task(pool, cmd.session_id, tid).await?;
        t.verify_submissible()?;
        t.verify_fencing(t.current_fencing_token)?;
        t.verify_fencing(t.current_fencing_token)?;
        t.verify_fencing(t.current_fencing_token)?;
        let evt = EventEnvelope::new("FindingSubmitted", cmd.session_id, cmd.tenant_id,
            cmd.requested_by.clone(),
            EventPayload::FindingSubmitted(openforce_domain::event::FindingSubmittedPayload {
                finding_type: "generic".into(), finding_data: serde_json::json!({}),
            })).with_task(tid, t.task_attempt, 0, t.plan_epoch);
        let new_ver = EventStore::append_events(pool, cmd.session_id, cmd.expected_version, &[evt]).await?;
        ProjectionBuilder::rebuild(pool, cmd.session_id).await?;
        Ok(Self::res(cmd.command_id, new_ver, serde_json::json!({"task_id": tid.to_string()})))
    }

    async fn request_effect(pool: &PgPool, cmd: &Command) -> DomainResult<CommandResult> {
        let evt = EventEnvelope::new("EffectRequested", cmd.session_id, cmd.tenant_id,
            cmd.requested_by.clone(),
            EventPayload::EffectRequested(openforce_domain::event::EffectRequestedPayload {
                effect_id: Uuid::now_v7().to_string(), effect_type: "generic".into(),
                idempotency_key: String::new(),
                requested_by_task: cmd.task_id.map(|id| id.to_string()).unwrap_or_default(),
                request_ref: String::new(), approval_policy: "default".into(),
            }));
        let new_ver = EventStore::append_events(pool, cmd.session_id, cmd.expected_version, &[evt]).await?;
        Ok(Self::res(cmd.command_id, new_ver, serde_json::json!({"status": "requested"})))
    }

    async fn mark_succeeded(pool: &PgPool, cmd: &Command) -> DomainResult<CommandResult> {
        let tid = cmd.task_id.ok_or(DomainError::ValidationFailed { detail: "task_id required".into() })?;
        let t = ProjectionRepo::get_task(pool, cmd.session_id, tid).await?;
        if !t.state.is_runnable() { return Err(DomainError::InvalidTransition {
            from: t.state.as_str().into(), to: "Succeeded".into()
        }); }
        let evt = EventEnvelope::new("TaskSucceeded", cmd.session_id, cmd.tenant_id,
            cmd.requested_by.clone(),
            EventPayload::TaskSucceeded(openforce_domain::event::TaskSucceededPayload {
                lease_id: t.current_lease_id.map(|id| id.to_string()).unwrap_or_default(),
            })).with_task(tid, t.task_attempt, 0, t.plan_epoch);
        let new_ver = EventStore::append_events(pool, cmd.session_id, cmd.expected_version, &[evt]).await?;
        ProjectionBuilder::rebuild(pool, cmd.session_id).await?;
        Ok(Self::res(cmd.command_id, new_ver, serde_json::json!({"task_id": tid.to_string(), "state": "Succeeded"})))
    }

    async fn mark_timed_out(pool: &PgPool, cmd: &Command) -> DomainResult<CommandResult> {
        let tid = cmd.task_id.ok_or(DomainError::ValidationFailed { detail: "task_id required".into() })?;
        let t = ProjectionRepo::get_task(pool, cmd.session_id, tid).await?;
        if !t.state.is_runnable() { return Err(DomainError::InvalidTransition {
            from: t.state.as_str().into(), to: "TimedOut".into()
        }); }
        let evt = EventEnvelope::new("TaskTimedOut", cmd.session_id, cmd.tenant_id,
            cmd.requested_by.clone(),
            EventPayload::TaskTimedOut(openforce_domain::event::TaskTimedOutPayload {
                lease_id: t.current_lease_id.map(|id| id.to_string()).unwrap_or_default(),
                reason: "lease expired".into(),
            })).with_task(tid, t.task_attempt, 0, t.plan_epoch);
        let new_ver = EventStore::append_events(pool, cmd.session_id, cmd.expected_version, &[evt]).await?;
        ProjectionBuilder::rebuild(pool, cmd.session_id).await?;
        Ok(Self::res(cmd.command_id, new_ver, serde_json::json!({"task_id": tid.to_string(), "state": "TimedOut"})))
    }

    async fn replan(pool: &PgPool, cmd: &Command) -> DomainResult<CommandResult> {
        let s = ProjectionRepo::get_session(pool, cmd.session_id).await?;
        let nv = s.current_plan_version + 1; let ne = s.current_plan_epoch + 1;
        let evts = vec![
            EventEnvelope::new("PlanEpochStarted", cmd.session_id, cmd.tenant_id,
                cmd.requested_by.clone(),
                EventPayload::PlanEpochStarted(openforce_domain::event::PlanEpochStartedPayload {
                    plan_epoch: ne, plan_version: nv,
                })),
            EventEnvelope::new("PlanCompiled", cmd.session_id, cmd.tenant_id,
                cmd.requested_by.clone(),
                EventPayload::PlanCompiled(openforce_domain::event::PlanCompiledPayload {
                    plan_version: nv, plan_epoch: ne, tasks: vec![], dag_edges: vec![],
                })),
        ];
        let new_ver = EventStore::append_events(pool, cmd.session_id, cmd.expected_version, &evts).await?;
        ProjectionBuilder::rebuild(pool, cmd.session_id).await?;
        Ok(Self::res(cmd.command_id, new_ver, serde_json::json!({"new_plan_version": nv, "new_plan_epoch": ne})))
    }

    async fn cancel(pool: &PgPool, cmd: &Command) -> DomainResult<CommandResult> {
        let tid = cmd.task_id.ok_or(DomainError::ValidationFailed { detail: "task_id required".into() })?;
        let t = ProjectionRepo::get_task(pool, cmd.session_id, tid).await?;
        t.state.can_transition_to(TaskState::Cancelled)?;
        let evt = EventEnvelope::new("TaskCancelled", cmd.session_id, cmd.tenant_id,
            cmd.requested_by.clone(),
            EventPayload::TaskCancelled(openforce_domain::event::TaskCancelledPayload {
                reason: "user cancelled".into(),
            })).with_task(tid, t.task_attempt, 0, t.plan_epoch);
        let new_ver = EventStore::append_events(pool, cmd.session_id, cmd.expected_version, &[evt]).await?;
        ProjectionBuilder::rebuild(pool, cmd.session_id).await?;
        Ok(Self::res(cmd.command_id, new_ver, serde_json::json!({"task_id": tid.to_string(), "state": "Cancelled"})))
    }
}

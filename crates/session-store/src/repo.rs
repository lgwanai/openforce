use sqlx::PgPool; use uuid::Uuid;
use openforce_domain::error::{DomainError, DomainResult};
use openforce_domain::session::{Session, SessionState};
use openforce_domain::task::{Task, TaskState, ReplanDisposition};

pub struct ProjectionRepo;

impl ProjectionRepo {
    pub async fn get_session(pool: &PgPool, session_id: Uuid) -> DomainResult<Session> {
        let row = sqlx::query_as::<_, SessionRow>(
            "SELECT session_id, tenant_id, goal, state, current_plan_version, current_plan_epoch,
                    session_version, policy_profile, created_at, updated_at
             FROM session_projection WHERE session_id = $1"
        ).bind(session_id).fetch_optional(pool).await
         .map_err(|e| DomainError::ValidationFailed { detail: e.to_string() })?;
        row.map(|r| Session {
            session_id: r.session_id, tenant_id: r.tenant_id, goal: r.goal,
            state: SessionState::from_str(&r.state).unwrap_or(SessionState::Active),
            current_plan_version: r.current_plan_version, current_plan_epoch: r.current_plan_epoch,
            session_version: r.session_version,
            policy_profile: serde_json::from_value(r.policy_profile).unwrap_or_default(),
            created_at: r.created_at, updated_at: r.updated_at,
        }).ok_or_else(|| DomainError::SessionNotFound { session_id: session_id.to_string() })
    }

    pub async fn get_task(pool: &PgPool, session_id: Uuid, task_id: Uuid) -> DomainResult<Task> {
        let row = sqlx::query_as::<_, TaskRow>(
            "SELECT session_id, task_id, task_type, state, task_attempt, current_lease_id,
                    current_fencing_token, current_worker_spec_id, plan_epoch,
                    replan_disposition, dependency_ids, downstream_ids, updated_at
             FROM task_projection WHERE session_id = $1 AND task_id = $2"
        ).bind(session_id).bind(task_id).fetch_optional(pool).await
         .map_err(|e| DomainError::ValidationFailed { detail: e.to_string() })?;
        row.map(|r| {
            let upstream: Vec<Uuid> = serde_json::from_value(r.dependency_ids).unwrap_or_default();
            let downstream: Vec<Uuid> = serde_json::from_value(r.downstream_ids).unwrap_or_default();
            Task {
                session_id: r.session_id, task_id: r.task_id, task_type: r.task_type,
                state: TaskState::from_str(&r.state).unwrap_or(TaskState::Pending),
                task_attempt: r.task_attempt,
                current_lease_id: r.current_lease_id,
                current_fencing_token: r.current_fencing_token as u64,
                current_worker_spec_id: r.current_worker_spec_id,
                plan_epoch: r.plan_epoch,
                replan_disposition: ReplanDisposition::from_str(&r.replan_disposition)
                    .unwrap_or(ReplanDisposition::Active),
                upstream_task_ids: upstream, downstream_task_ids: downstream,
                created_at: r.updated_at, updated_at: r.updated_at,
            }
        }).ok_or_else(|| DomainError::TaskNotFound { task_id: task_id.to_string() })
    }

    pub async fn list_tasks(pool: &PgPool, session_id: Uuid, state_filter: Option<String>)
        -> DomainResult<Vec<Task>>
    {
        let rows = if let Some(ref s) = state_filter {
            sqlx::query_as::<_, TaskRow>(
                "SELECT session_id, task_id, task_type, state, task_attempt, current_lease_id,
                        current_fencing_token, current_worker_spec_id, plan_epoch,
                        replan_disposition, dependency_ids, downstream_ids, updated_at
                 FROM task_projection WHERE session_id = $1 AND state = $2"
            ).bind(session_id).bind(s).fetch_all(pool).await
        } else {
            sqlx::query_as::<_, TaskRow>(
                "SELECT session_id, task_id, task_type, state, task_attempt, current_lease_id,
                        current_fencing_token, current_worker_spec_id, plan_epoch,
                        replan_disposition, dependency_ids, downstream_ids, updated_at
                 FROM task_projection WHERE session_id = $1"
            ).bind(session_id).fetch_all(pool).await
        }.map_err(|e| DomainError::ValidationFailed { detail: e.to_string() })?;
        rows.into_iter().map(|r| {
            let upstream: Vec<Uuid> = serde_json::from_value(r.dependency_ids).unwrap_or_default();
            let downstream: Vec<Uuid> = serde_json::from_value(r.downstream_ids).unwrap_or_default();
            Ok(Task {
                session_id: r.session_id, task_id: r.task_id, task_type: r.task_type,
                state: TaskState::from_str(&r.state).unwrap_or(TaskState::Pending),
                task_attempt: r.task_attempt,
                current_lease_id: r.current_lease_id,
                current_fencing_token: r.current_fencing_token as u64,
                current_worker_spec_id: r.current_worker_spec_id,
                plan_epoch: r.plan_epoch,
                replan_disposition: ReplanDisposition::from_str(&r.replan_disposition)
                    .unwrap_or(ReplanDisposition::Active),
                upstream_task_ids: upstream, downstream_task_ids: downstream,
                created_at: r.updated_at, updated_at: r.updated_at,
            })
        }).collect()
    }
}

#[derive(Debug, sqlx::FromRow)] struct SessionRow {
    session_id: Uuid, tenant_id: Uuid, goal: String, state: String,
    current_plan_version: i32, current_plan_epoch: i32, session_version: i64,
    policy_profile: serde_json::Value,
    created_at: chrono::DateTime<chrono::Utc>, updated_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Debug, sqlx::FromRow)] struct TaskRow {
    session_id: Uuid, task_id: Uuid, task_type: String, state: String,
    task_attempt: i32, current_lease_id: Option<Uuid>, current_fencing_token: i64,
    current_worker_spec_id: Option<Uuid>, plan_epoch: i32, replan_disposition: String,
    dependency_ids: serde_json::Value, downstream_ids: serde_json::Value,
    updated_at: chrono::DateTime<chrono::Utc>,
}

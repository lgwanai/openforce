use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;
use openforce_domain::error::{DomainError, DomainResult};
use openforce_domain::event::EventEnvelope;

pub struct EventStore;

impl EventStore {
    pub async fn append_events(
        pool: &PgPool, session_id: Uuid, expected_version: i64, events: &[EventEnvelope],
    ) -> DomainResult<i64> {
        if events.is_empty() { return Ok(expected_version); }
        let mut tx = pool.begin().await.map_err(|e| DomainError::ValidationFailed {
            detail: format!("begin tx: {e}")
        })?;
        let lock_key = session_id.as_u64_pair();
        sqlx::query("SELECT pg_advisory_xact_lock($1, $2)")
            .bind(lock_key.0 as i32).bind(lock_key.1 as i32)
            .execute(&mut *tx).await.map_err(|e| DomainError::ValidationFailed {
                detail: format!("lock: {e}")
            })?;
        let current: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(session_version), 0) FROM event_log WHERE session_id = $1"
        ).bind(session_id).fetch_one(&mut *tx).await.map_err(|e| DomainError::ValidationFailed {
            detail: format!("select: {e}")
        })?;
        
        if current != expected_version {
            tx.rollback().await.ok();
            return Err(DomainError::VersionConflict { expected: expected_version, actual: current });
        }
        let mut next = current + 1;
        for event in events {
            sqlx::query(
                "INSERT INTO event_log (event_id, event_type, session_id, tenant_id,
                 plan_version, plan_epoch, task_id, task_attempt, causation_id,
                 correlation_id, producer_component, producer_instance, producer_region,
                 session_version, payload, occurred_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)"
            ).bind(event.event_id).bind(&event.event_type).bind(session_id)
             .bind(event.tenant_id).bind(event.plan_version).bind(event.plan_epoch)
             .bind(event.task_id).bind(event.task_attempt).bind(event.causation_id)
             .bind(event.correlation_id).bind(&event.producer.component)
             .bind(&event.producer.instance_id).bind(&event.producer.region)
             .bind(next).bind(serde_json::to_value(&event.payload).map_err(|e| {
                DomainError::ValidationFailed { detail: format!("ser: {e}") }
             })?).bind(event.occurred_at)
             .execute(&mut *tx).await.map_err(|e| DomainError::ValidationFailed {
                detail: format!("insert: {e}")
             })?;
            next += 1;
        }
        tx.commit().await.map_err(|e| DomainError::ValidationFailed {
            detail: format!("commit: {e}")
        })?;
        Ok(next - 1)
    }

    pub async fn read_event_log(
        pool: &PgPool, session_id: Uuid, from_version: i64, max_events: i32,
    ) -> DomainResult<Vec<EventEnvelope>> {
        let limit = if max_events > 0 { max_events as i64 } else { i64::MAX };
        let rows = sqlx::query_as::<_, EventLogRow>(
            "SELECT event_id, event_type, session_id, tenant_id, plan_version, plan_epoch,
                    task_id, task_attempt, causation_id, correlation_id,
                    producer_component, producer_instance, producer_region,
                    session_version, payload, occurred_at
             FROM event_log WHERE session_id = $1 AND session_version > $2
             ORDER BY session_version ASC LIMIT $3"
        ).bind(session_id).bind(from_version).bind(limit)
         .fetch_all(pool).await.map_err(|e| DomainError::ValidationFailed {
            detail: format!("read: {e}")
        })?;
        rows.into_iter().map(|r| r.to_envelope()).collect()
    }
}

#[derive(Debug, sqlx::FromRow)]
struct EventLogRow {
    event_id: Uuid, event_type: String, session_id: Uuid, tenant_id: Uuid,
    plan_version: i32, plan_epoch: i32, task_id: Option<Uuid>, task_attempt: Option<i32>,
    causation_id: Uuid, correlation_id: Uuid,
    producer_component: String, producer_instance: String, producer_region: String,
    session_version: i64, payload: serde_json::Value, occurred_at: chrono::DateTime<Utc>,
}

impl EventLogRow {
    fn to_envelope(self) -> DomainResult<EventEnvelope> {
        let payload: openforce_domain::event::EventPayload = serde_json::from_value(self.payload)
            .map_err(|e| DomainError::ValidationFailed { detail: format!("deser: {e}") })?;
        Ok(EventEnvelope {
            event_id: self.event_id, event_type: self.event_type,
            session_id: self.session_id, tenant_id: self.tenant_id,
            plan_version: self.plan_version as i64, plan_epoch: self.plan_epoch,
            task_id: self.task_id, task_attempt: self.task_attempt.unwrap_or(0),
            producer: openforce_domain::event::ProducerIdentity {
                component: self.producer_component, instance_id: self.producer_instance,
                region: self.producer_region,
            },
            causation_id: self.causation_id, correlation_id: self.correlation_id,
            occurred_at: self.occurred_at, session_version: self.session_version, payload,
        })
    }
}

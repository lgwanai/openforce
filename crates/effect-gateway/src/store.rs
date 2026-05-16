use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;
use openforce_domain::error::{DomainError, DomainResult};
use openforce_domain::effect::EffectState;

pub struct EffectStore { pool: PgPool }

impl EffectStore {
    pub fn new(pool: PgPool) -> Self { Self { pool } }

    pub async fn request_effect(
        &self, effect_id: Uuid, session_id: Uuid, task_id: Option<Uuid>,
        tenant_id: Uuid, effect_type: &str, target: &str,
        idempotency_key: &str, payload_ref: &str,
    ) -> DomainResult<(Uuid, EffectState)> {
        // Atomic INSERT with ON CONFLICT — eliminates TOCTOU race
        let now = Utc::now();
        let row = sqlx::query_as::<_, (Uuid, String)>(
            "INSERT INTO effects (effect_id, session_id, task_id, tenant_id, effect_type,
             target, idempotency_key, payload_ref, status, created_at, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
             ON CONFLICT (idempotency_key) DO UPDATE SET idempotency_key = EXCLUDED.idempotency_key
             RETURNING effect_id, status"
        ).bind(effect_id).bind(session_id).bind(task_id).bind(tenant_id)
         .bind(effect_type).bind(target).bind(idempotency_key).bind(payload_ref)
         .bind(EffectState::Requested.as_str()).bind(now).bind(now)
         .fetch_one(&self.pool).await.map_err(|e| DomainError::ValidationFailed { detail: format!("insert effect: {e}") })?;
        let state = EffectState::from_str(&row.1).unwrap_or(EffectState::Requested);
        Ok((row.0, state))
    }

    pub async fn approve_effect(&self, effect_id: Uuid, approved_by: &str) -> DomainResult<EffectState> {
        let mut tx = self.pool.begin().await.map_err(|e| DomainError::ValidationFailed { detail: format!("begin tx: {e}") })?;
        let now = Utc::now();
        let rows = sqlx::query(
            "UPDATE effects SET status = $2, approved_by = $3, updated_at = $4
             WHERE effect_id = $1 AND status = 'requested'"
        ).bind(effect_id).bind(EffectState::Approved.as_str()).bind(approved_by).bind(now)
         .execute(&mut *tx).await.map_err(|e| DomainError::ValidationFailed { detail: e.to_string() })?;
        if rows.rows_affected() == 0 {
            tx.rollback().await.ok();
            return Err(DomainError::ValidationFailed { detail: "effect not in requested state".into() });
        }
        sqlx::query(
            "INSERT INTO effect_outbox (effect_id, status, created_at)
             VALUES ($1, 'pending', $2)"
        ).bind(effect_id).bind(now).execute(&mut *tx).await.map_err(|e| DomainError::ValidationFailed { detail: e.to_string() })?;
        tx.commit().await.map_err(|e| DomainError::ValidationFailed { detail: format!("commit tx: {e}") })?;
        Ok(EffectState::Approved)
    }

    pub async fn commit_effect(&self, effect_id: Uuid, execution_ref: &str) -> DomainResult<EffectState> {
        let now = Utc::now();
        sqlx::query(
            "UPDATE effects SET status = $2, execution_ref = $3, updated_at = $4
             WHERE effect_id = $1 AND status IN ('approved', 'executing')"
        ).bind(effect_id).bind(EffectState::Committed.as_str()).bind(execution_ref).bind(now)
         .execute(&self.pool).await.map_err(|e| DomainError::ValidationFailed { detail: e.to_string() })?;
        sqlx::query(
            "UPDATE effect_outbox SET status = 'dispatched', dispatched_at = $2 WHERE effect_id = $1"
        ).bind(effect_id).bind(now).execute(&self.pool).await.ok();
        Ok(EffectState::Committed)
    }

    pub async fn get_effect(&self, effect_id: Uuid) -> DomainResult<(String, String, Option<String>)> {
        let row = sqlx::query_as::<_, (String, String, Option<String>)>(
            "SELECT status, idempotency_key, execution_ref FROM effects WHERE effect_id = $1"
        ).bind(effect_id).fetch_optional(&self.pool).await.map_err(|e| DomainError::ValidationFailed { detail: e.to_string() })?;
        row.ok_or(DomainError::ValidationFailed { detail: "effect not found".into() })
    }
}


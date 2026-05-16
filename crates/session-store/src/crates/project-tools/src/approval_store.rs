use chrono::{DateTime, Utc};
use sha2::{Sha256, Digest};
use sqlx::PgPool;
use uuid::Uuid;
use openforce_domain::error::{DomainError, DomainResult};
use openforce_domain::approval::{ApprovalRequest, ApprovalToken, ApprovalStatus};
use openforce_domain::patch::PatchClassification;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};

/// ApprovalStore manages HITL approval lifecycle (architecture doc section 6.6-6.8)
pub struct ApprovalStore {
    pool: PgPool,
    signing_key: Ed25519KeyPair,
}

impl ApprovalStore {
    pub fn new(pool: PgPool) -> Self {
        let rng = SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).expect("generate signing key");
        let key = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("parse signing key");
        Self { pool, signing_key: key }
    }

    pub async fn create_approval_request(
        &self,
        session_id: Uuid, task_id: Uuid, task_attempt: i32,
        lease_id: Uuid, fencing_token: u64, worker_spec_id: Uuid,
        tool_name: &str, target_paths: &[String],
        base_snapshot_id: &str, payload_sha256: &str,
        classification: &PatchClassification,
        ttl_minutes: i32,
    ) -> DomainResult<ApprovalRequest> {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::minutes(ttl_minutes as i64);
        let request_id = Uuid::now_v7();

        sqlx::query(
            "INSERT INTO approval_requests (
                approval_request_id, session_id, task_id, task_attempt,
                lease_id, fencing_token, worker_spec_id, tool_name,
                patch_risk_level, target_paths, base_snapshot_id,
                payload_sha256, status, created_at, expires_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)"
        )
        .bind(request_id).bind(session_id).bind(task_id).bind(task_attempt)
        .bind(lease_id).bind(fencing_token as i64).bind(worker_spec_id)
        .bind(tool_name).bind(classification.risk_level.as_str())
        .bind(serde_json::to_value(target_paths).unwrap_or_default())
        .bind(base_snapshot_id).bind(payload_sha256)
        .bind(ApprovalStatus::PendingHuman.as_str())
        .bind(now).bind(expires_at)
        .execute(&self.pool).await.map_err(|e| DomainError::ValidationFailed {
            detail: format!("insert approval: {e}")
        })?;

        Ok(ApprovalRequest {
            approval_request_id: request_id, command_id: Uuid::nil(),
            session_id, task_id, task_attempt, lease_id, fencing_token,
            worker_spec_id, tool_name: tool_name.into(),
            patch_risk_level: classification.risk_level.as_str().into(),
            reason_codes: classification.reason_codes.iter().map(|r| r.as_str().into()).collect(),
            target_paths: target_paths.to_vec(),
            base_snapshot_id: base_snapshot_id.into(),
            payload_sha256: payload_sha256.into(),
            status: ApprovalStatus::PendingHuman,
            created_at: now, expires_at,
            resolved_at: None, approved_by: None, rejected_by: None, rejected_reason: None,
        })
    }

    pub async fn approve_request(
        &self, approval_request_id: Uuid, approver_id: &str, usage_limit: u32,
    ) -> DomainResult<ApprovalToken> {
        // Fetch the pending request
        let req = sqlx::query_as::<_, ApprovalRequestRow>(
            "SELECT approval_request_id, session_id, task_id, task_attempt,
                    lease_id, fencing_token, worker_spec_id, tool_name,
                    patch_risk_level, target_paths, base_snapshot_id,
                    payload_sha256, status, created_at, expires_at
             FROM approval_requests WHERE approval_request_id = $1 AND status = 'pending_human'"
        ).bind(approval_request_id).fetch_optional(&self.pool).await
         .map_err(|e| DomainError::ValidationFailed { detail: e.to_string() })?
         .ok_or(DomainError::ValidationFailed { detail: "approval request not found".into() })?;

        let now = Utc::now();
        let token_id = Uuid::now_v7();
        let expires_at = now + chrono::Duration::minutes(30);

        // Sign the token binding
        let binding_data = serde_json::json!({
            "approval_request_id": approval_request_id.to_string(),
            "approval_token_id": token_id.to_string(),
            "session_id": req.session_id.to_string(),
            "task_id": req.task_id.to_string(),
            "task_attempt": req.task_attempt,
            "lease_id": req.lease_id.to_string(),
            "fencing_token": req.fencing_token,
            "worker_spec_id": req.worker_spec_id.to_string(),
            "tool_name": req.tool_name,
            "target_paths": req.target_paths,
            "base_snapshot_id": req.base_snapshot_id,
            "payload_sha256": req.payload_sha256,
            "issued_at": now.to_rfc3339(),
            "expires_at": expires_at.to_rfc3339(),
            "approved_by": approver_id,
            "usage_limit": usage_limit,
        });
        let binding_json = serde_json::to_vec(&binding_data).unwrap();
        let signature = self.signing_key.sign(&binding_json).as_ref().to_vec();

        // Update request status
        sqlx::query(
            "UPDATE approval_requests SET status = 'approved', approved_by = $2, resolved_at = $3
             WHERE approval_request_id = $1"
        ).bind(approval_request_id).bind(approver_id).bind(now)
         .execute(&self.pool).await.map_err(|e| DomainError::ValidationFailed { detail: e.to_string() })?;

        // Insert token
        sqlx::query(
            "INSERT INTO approval_tokens (
                approval_token_id, approval_request_id, session_id, task_id, task_attempt,
                lease_id, fencing_token, worker_spec_id, tool_name, target_paths,
                base_snapshot_id, payload_sha256, status, usage_limit, usage_count,
                issued_at, expires_at, approved_by, signature
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)"
        ).bind(token_id).bind(approval_request_id).bind(req.session_id).bind(req.task_id)
         .bind(req.task_attempt).bind(req.lease_id).bind(req.fencing_token)
         .bind(req.worker_spec_id).bind(&req.tool_name).bind(&req.target_paths)
         .bind(&req.base_snapshot_id).bind(&req.payload_sha256)
         .bind(ApprovalStatus::Approved.as_str()).bind(usage_limit as i32).bind(0i32)
         .bind(now).bind(expires_at).bind(approver_id).bind(&signature)
         .execute(&self.pool).await.map_err(|e| DomainError::ValidationFailed { detail: e.to_string() })?;

        Ok(ApprovalToken {
            approval_token_id: token_id, approval_request_id,
            session_id: req.session_id, task_id: req.task_id,
            task_attempt: req.task_attempt, lease_id: req.lease_id,
            fencing_token: req.fencing_token as u64,
            worker_spec_id: req.worker_spec_id,
            tool_name: req.tool_name, target_paths: serde_json::from_value(req.target_paths).unwrap_or_default(),
            base_snapshot_id: req.base_snapshot_id, payload_sha256: req.payload_sha256,
            status: ApprovalStatus::Approved, usage_limit, usage_count: 0,
            issued_at: now, expires_at, approved_by: approver_id.into(), signature,
        })
    }

    pub async fn consume_token(
        &self, token_id: Uuid,
        session_id: Uuid, task_id: Uuid, task_attempt: i32,
        lease_id: Uuid, fencing_token: u64,
        base_snapshot_id: &str, payload_sha256: &str,
    ) -> DomainResult<ApprovalToken> {
        let token = sqlx::query_as::<_, ApprovalTokenRow>(
            "SELECT approval_token_id, approval_request_id, session_id, task_id, task_attempt,
                    lease_id, fencing_token, worker_spec_id, tool_name, target_paths,
                    base_snapshot_id, payload_sha256, status, usage_limit, usage_count,
                    issued_at, expires_at, approved_by, signature
             FROM approval_tokens WHERE approval_token_id = $1 AND status = 'approved'"
        ).bind(token_id).fetch_optional(&self.pool).await
         .map_err(|e| DomainError::ValidationFailed { detail: e.to_string() })?
         .ok_or(DomainError::ValidationFailed { detail: "token not found".into() })?;

        let token = ApprovalToken {
            approval_token_id: token.approval_token_id,
            approval_request_id: token.approval_request_id,
            session_id: token.session_id, task_id: token.task_id,
            task_attempt: token.task_attempt, lease_id: token.lease_id,
            fencing_token: token.fencing_token as u64,
            worker_spec_id: token.worker_spec_id,
            tool_name: token.tool_name,
            target_paths: serde_json::from_value(token.target_paths).unwrap_or_default(),
            base_snapshot_id: token.base_snapshot_id, payload_sha256: token.payload_sha256,
            status: ApprovalStatus::Approved, usage_limit: token.usage_limit as u32,
            usage_count: token.usage_count as u32,
            issued_at: token.issued_at, expires_at: token.expires_at,
            approved_by: token.approved_by, signature: token.signature,
        };

        // Verify binding
        token.verify_binding(session_id, task_id, task_attempt, lease_id, fencing_token,
            base_snapshot_id, payload_sha256)?;

        if Utc::now() >= token.expires_at {
            return Err(DomainError::ValidationFailed { detail: "token expired".into() });
        }

        // Increment usage count atomically
        sqlx::query(
            "UPDATE approval_tokens SET usage_count = usage_count + 1,
                    status = CASE WHEN usage_count + 1 >= usage_limit THEN 'consumed' ELSE 'approved' END
             WHERE approval_token_id = $1 AND usage_count < usage_limit"
        ).bind(token_id).execute(&self.pool).await
         .map_err(|e| DomainError::ValidationFailed { detail: e.to_string() })?;

        Ok(token)
    }
}

#[derive(Debug, sqlx::FromRow)]
struct ApprovalRequestRow {
    approval_request_id: Uuid, session_id: Uuid, task_id: Uuid,
    task_attempt: i32, lease_id: Uuid, fencing_token: i64,
    worker_spec_id: Uuid, tool_name: String, patch_risk_level: String,
    target_paths: serde_json::Value, base_snapshot_id: String,
    payload_sha256: String, status: String,
    created_at: DateTime<Utc>, expires_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct ApprovalTokenRow {
    approval_token_id: Uuid, approval_request_id: Uuid,
    session_id: Uuid, task_id: Uuid, task_attempt: i32,
    lease_id: Uuid, fencing_token: i64, worker_spec_id: Uuid,
    tool_name: String, target_paths: serde_json::Value,
    base_snapshot_id: String, payload_sha256: String,
    status: String, usage_limit: i32, usage_count: i32,
    issued_at: DateTime<Utc>, expires_at: DateTime<Utc>,
    approved_by: String, signature: Vec<u8>,
}

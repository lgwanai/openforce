use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{DomainError, DomainResult};

/// Monotonically increasing fencing token per task.
/// A new wrapping type for type safety.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FencingToken(pub u64);

impl FencingToken {
    pub fn new() -> Self { Self(1) }

    pub fn next(self) -> Self { Self(self.0 + 1) }

    pub fn value(&self) -> u64 { self.0 }
}

impl Default for FencingToken {
    fn default() -> Self { Self(1) }
}

impl std::fmt::Display for FencingToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeaseState {
    Active,
    Expired,
    Renewed,
    Revoked,
}

impl LeaseState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Expired => "expired",
            Self::Renewed => "renewed",
            Self::Revoked => "revoked",
        }
    }
}

/// A lease grants a worker temporary write authority for a specific task attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lease {
    pub lease_id: Uuid,
    pub session_id: Uuid,
    pub task_id: Uuid,
    pub task_attempt: i32,
    pub fencing_token: u64,
    pub worker_spec_id: Uuid,
    pub state: LeaseState,
    pub expire_at: DateTime<Utc>,
    pub renewal_deadline: Option<DateTime<Utc>>,
    pub heartbeat_interval_sec: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Lease {
    pub fn new(
        session_id: Uuid,
        task_id: Uuid,
        task_attempt: i32,
        fencing_token: u64,
        worker_spec_id: Uuid,
        ttl: Duration,
        heartbeat_interval_sec: i32,
    ) -> Self {
        let now = Utc::now();
        let expire_at = now + ttl;
        let renewal_deadline = expire_at - Duration::seconds(heartbeat_interval_sec as i64 * 2);
        Self {
            lease_id: Uuid::now_v7(),
            session_id,
            task_id,
            task_attempt,
            fencing_token,
            worker_spec_id,
            state: LeaseState::Active,
            expire_at,
            renewal_deadline: Some(renewal_deadline),
            heartbeat_interval_sec,
            created_at: now,
            updated_at: now,
        }
    }

    /// Check if the lease has expired
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expire_at
    }

    /// Check if the given fencing token is still valid
    pub fn verify_fencing(&self, provided: u64) -> DomainResult<()> {
        if provided < self.fencing_token {
            return Err(DomainError::FencingTokenStale {
                provided,
                current: self.fencing_token,
            });
        }
        Ok(())
    }

    /// Verify the lease is still active and not expired
    pub fn verify_active(&self) -> DomainResult<()> {
        if self.state != LeaseState::Active {
            return Err(DomainError::LeaseExpired);
        }
        if self.is_expired() {
            return Err(DomainError::LeaseExpired);
        }
        Ok(())
    }

    /// Renew the lease, extending its expiry
    pub fn renew(&mut self, extension: Duration) -> DomainResult<()> {
        self.verify_active()?;
        self.expire_at = Utc::now() + extension;
        self.renewal_deadline = Some(self.expire_at - Duration::seconds(self.heartbeat_interval_sec as i64 * 2));
        self.state = LeaseState::Renewed;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Mark the lease as expired
    pub fn mark_expired(&mut self) {
        self.state = LeaseState::Expired;
        self.updated_at = Utc::now();
    }

    /// Mark the lease as revoked
    pub fn revoke(&mut self) {
        self.state = LeaseState::Revoked;
        self.updated_at = Utc::now();
    }
}

/// Default lease TTL: 5 minutes
pub const DEFAULT_LEASE_TTL_SECS: i64 = 300;

/// Default heartbeat interval: 15 seconds
pub const DEFAULT_HEARTBEAT_INTERVAL_SECS: i32 = 15;

#[cfg(test)]
mod tests {
    use super::*;

    fn test_lease() -> Lease {
        Lease::new(
            Uuid::now_v7(),
            Uuid::now_v7(),
            1,
            5,
            Uuid::now_v7(),
            Duration::seconds(DEFAULT_LEASE_TTL_SECS),
            DEFAULT_HEARTBEAT_INTERVAL_SECS,
        )
    }

    #[test]
    fn test_lease_not_expired_immediately() {
        let lease = test_lease();
        assert!(!lease.is_expired());
    }

    #[test]
    fn test_lease_fencing_rejection() {
        let lease = test_lease();
        assert!(lease.verify_fencing(4).is_err());
        assert!(lease.verify_fencing(5).is_ok());
        assert!(lease.verify_fencing(6).is_ok());
    }

    #[test]
    fn test_lease_active_verification() {
        let lease = test_lease();
        assert!(lease.verify_active().is_ok());
    }

    #[test]
    fn test_lease_renewal() {
        let mut lease = test_lease();
        let old_expire = lease.expire_at;
        let result = lease.renew(Duration::seconds(300));
        assert!(result.is_ok());
        assert!(lease.expire_at > old_expire);
    }

    #[test]
    fn test_expired_lease_rejects_renewal() {
        let mut lease = Lease::new(
            Uuid::now_v7(),
            Uuid::now_v7(),
            1,
            1,
            Uuid::now_v7(),
            Duration::seconds(-1), // immediately expired
            15,
        );
        assert!(lease.is_expired());
        assert!(lease.renew(Duration::seconds(300)).is_err());
    }
}

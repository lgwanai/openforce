use chrono::{DateTime, Utc, Duration};
use serde::{Serialize, Deserialize};
use uuid::Uuid;

/// Data retention policy per tenant (architecture doc section 28.2)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub tenant_id: Uuid,
    pub event_log_retention_days: u32,
    pub artifact_retention_days: u32,
    pub observer_sample_retention_days: u32,
    pub debug_log_retention_days: u32,
    pub backup_retention_days: u32,
    pub legal_hold: bool,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            tenant_id: Uuid::nil(),
            event_log_retention_days: 365,
            artifact_retention_days: 180,
            observer_sample_retention_days: 30,
            debug_log_retention_days: 14,
            backup_retention_days: 35,
            legal_hold: false,
        }
    }
}

impl RetentionPolicy {
    pub fn expires_at(&self, created_at: DateTime<Utc>, data_type: &str) -> DateTime<Utc> {
        if self.legal_hold { return DateTime::<Utc>::MAX_UTC; }
        let days = match data_type {
            "event_log" => self.event_log_retention_days,
            "artifact" => self.artifact_retention_days,
            "observer" => self.observer_sample_retention_days,
            "debug" => self.debug_log_retention_days,
            "backup" => self.backup_retention_days,
            _ => 30,
        };
        created_at + Duration::days(days as i64)
    }

    pub fn can_delete(&self, created_at: DateTime<Utc>, data_type: &str) -> bool {
        Utc::now() >= self.expires_at(created_at, data_type)
    }
}

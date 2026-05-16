use serde::{Serialize, Deserialize};
use uuid::Uuid;

/// Bring Your Own Key — tenant-managed encryption keys (architecture doc section 28.6)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KeyManagementMode {
    PlatformManaged,
    TenantDedicated,
    Byok,
    Hyok,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum KeyState {
    Active,
    Rotating,
    Disabled,
    Revoked,
    Destroyed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionKey {
    pub key_id: Uuid,
    pub tenant_id: Uuid,
    pub mode: KeyManagementMode,
    pub state: KeyState,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub rotated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub destroyed_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub struct ByokManager;

impl ByokManager {
    pub fn create_key(tenant_id: Uuid, mode: KeyManagementMode) -> EncryptionKey {
        EncryptionKey {
            key_id: Uuid::now_v7(), tenant_id, mode,
            state: KeyState::Active,
            created_at: chrono::Utc::now(),
            rotated_at: None, destroyed_at: None,
        }
    }

    pub fn rotate(&self, key: &mut EncryptionKey) {
        key.state = KeyState::Rotating;
        key.rotated_at = Some(chrono::Utc::now());
    }

    pub fn destroy(&self, key: &mut EncryptionKey) {
        key.state = KeyState::Destroyed;
        key.destroyed_at = Some(chrono::Utc::now());
    }

    pub fn is_usable(&self, key: &EncryptionKey) -> bool {
        matches!(key.state, KeyState::Active | KeyState::Rotating)
    }
}

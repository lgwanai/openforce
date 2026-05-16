use std::collections::HashSet;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KillSwitch {
    pub id: String,
    pub scope_type: KillScope,
    pub scope_value: String,
    pub actions: Vec<KillAction>,
    pub reason: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KillScope { Tenant, ModelEndpoint, Region, PromptBundle }

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KillAction { DenyNewSessions, DenyNewLeases, DenyModelEgress, DenyEffectRequests, FreezeBundle, QuarantineNetwork }

impl KillSwitch { pub fn expired(&self) -> bool { Utc::now() >= self.expires_at } }

#[derive(Default)]
pub struct KillSwitchRegistry {
    active: HashSet<(KillScope, String, KillAction)>,
}

impl KillSwitchRegistry {
    pub fn activate(&mut self, ks: &KillSwitch) {
        for a in &ks.actions {
            self.active.insert((ks.scope_type.clone(), ks.scope_value.clone(), a.clone()));
        }
    }
    pub fn deactivate(&mut self, ks: &KillSwitch) {
        for a in &ks.actions {
            self.active.remove(&(ks.scope_type.clone(), ks.scope_value.clone(), a.clone()));
        }
    }
    pub fn blocked(&self, scope: &KillScope, val: &str, action: &KillAction) -> bool {
        self.active.contains(&(scope.clone(), val.to_string(), action.clone()))
    }
}

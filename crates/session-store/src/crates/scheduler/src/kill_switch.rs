use openforce_policy_engine::kill_switch::{KillSwitchRegistry, KillSwitch, KillScope, KillAction};

pub struct SchedulerKillSwitch {
    registry: KillSwitchRegistry,
}

impl SchedulerKillSwitch {
    pub fn new() -> Self { Self { registry: KillSwitchRegistry::default() } }
    pub fn activate(&mut self, ks: KillSwitch) { self.registry.activate(&ks); }
    pub fn deactivate(&mut self, ks: &KillSwitch) { self.registry.deactivate(ks); }
    pub fn can_schedule(&self, tenant_id: &str) -> bool {
        !self.registry.blocked(&KillScope::Tenant, tenant_id, &KillAction::DenyNewLeases)
    }
    pub fn can_start_session(&self, tenant_id: &str) -> bool {
        !self.registry.blocked(&KillScope::Tenant, tenant_id, &KillAction::DenyNewSessions)
    }
}

use serde::{Serialize, Deserialize};

/// Release Gate Checklist — automated validation before launch.
/// Architecture doc section 29: emergency breakers, incident response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseGate {
    pub version: String,
    pub checks: Vec<GateCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateCheck {
    pub name: String,
    pub category: String,
    pub passed: Option<bool>,
    pub required: bool,
}

impl ReleaseGate {
    pub fn v1() -> Self {
        Self {
            version: "5.1".into(),
            checks: vec![
                GateCheck { name: "all_unit_tests_pass".into(), category: "test".into(), passed: None, required: true },
                GateCheck { name: "integration_tests_pass".into(), category: "test".into(), passed: None, required: true },
                GateCheck { name: "red_team_scenarios_clear".into(), category: "security".into(), passed: None, required: true },
                GateCheck { name: "migrations_reversible".into(), category: "data".into(), passed: None, required: true },
                GateCheck { name: "kill_switch_verified".into(), category: "ops".into(), passed: None, required: true },
                GateCheck { name: "approval_flow_tested".into(), category: "compliance".into(), passed: None, required: true },
                GateCheck { name: "tenant_isolation_verified".into(), category: "security".into(), passed: None, required: true },
                GateCheck { name: "effect_gateway_tested".into(), category: "integration".into(), passed: None, required: true },
            ],
        }
    }

    pub fn all_required_passed(&self) -> bool {
        self.checks.iter()
            .filter(|c| c.required)
            .all(|c| c.passed == Some(true))
    }

    pub fn mark(&mut self, name: &str, passed: bool) {
        if let Some(c) = self.checks.iter_mut().find(|c| c.name == name) {
            c.passed = Some(passed);
        }
    }
}

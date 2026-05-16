use crate::release_gate::ReleaseGate;
use crate::red_team::RedTeamScenario;

/// Go/No-Go release decision system.
/// Architecture doc section 25: launch validation.
pub struct GoNoGo {
    pub release_gate: ReleaseGate,
    pub red_team_results: Vec<RedTeamScenario>,
}

impl GoNoGo {
    pub fn new(gate: ReleaseGate, red_team: Vec<RedTeamScenario>) -> Self {
        Self { release_gate: gate, red_team_results: red_team }
    }

    pub fn decide(&self) -> ReleaseDecision {
        let gates_pass = self.release_gate.all_required_passed();
        let red_team_all_pass = self.red_team_results.iter()
            .filter_map(|s| s.result.as_ref())
            .all(|r| r.passed);

        if gates_pass && red_team_all_pass {
            ReleaseDecision::Go
        } else if gates_pass {
            ReleaseDecision::ConditionalGo {
                unresolved: self.red_team_results.iter()
                    .filter(|s| s.result.as_ref().map_or(true, |r| !r.passed))
                    .map(|s| s.name.clone())
                    .collect(),
            }
        } else {
            ReleaseDecision::NoGo
        }
    }
}

pub enum ReleaseDecision {
    Go,
    ConditionalGo { unresolved: Vec<String> },
    NoGo,
}

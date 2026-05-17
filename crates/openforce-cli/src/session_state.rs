use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;
use openforce_domain::session::SessionState;
use openforce_domain::session_phase::{SessionPhase, ConfirmationGate};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerOutput {
    pub worker_id: String, pub role: String, pub status: String, pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseResult {
    pub phase: SessionPhase, pub tasks_total: usize, pub tasks_ok: usize,
    pub worker_outputs: Vec<WorkerOutput>, pub plan_epoch: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingGate {
    pub gate_id: Uuid, pub phase: SessionPhase,
    pub artifact_summary: String, pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalSessionState {
    pub session_id: Uuid, pub goal: String, pub state: SessionState,
    pub current_phase: SessionPhase, pub plan_version: i32, pub plan_epoch: i32,
    pub workspace: PathBuf, pub pending_gate: Option<PendingGate>,
    pub phase_results: Vec<PhaseResult>, pub last_summary: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>, pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: Uuid, pub goal: String, pub state: SessionState,
    pub current_phase: SessionPhase, pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl LocalSessionState {
    pub fn create(goal: String, workspace: PathBuf) -> Self {
        let now = chrono::Utc::now();
        Self { session_id: Uuid::now_v7(), goal, state: SessionState::Active,
            current_phase: SessionPhase::Understand, plan_version: 0, plan_epoch: 1,
            workspace, pending_gate: None, phase_results: vec![], last_summary: None,
            created_at: now, updated_at: now }
    }

    fn state_dir(workspace: &PathBuf) -> PathBuf { workspace.join(".openforce").join("sessions") }

    fn state_path(workspace: &PathBuf, sid: &Uuid) -> PathBuf {
        Self::state_dir(workspace).join(format!("{sid}.json"))
    }

    pub fn save(&self) -> Result<(), String> {
        let dir = Self::state_dir(&self.workspace);
        std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir: {e}"))?;
        let json = serde_json::to_string_pretty(self).map_err(|e| format!("json: {e}"))?;
        std::fs::write(Self::state_path(&self.workspace, &self.session_id), json)
            .map_err(|e| format!("write: {e}"))
    }

    pub fn load(workspace: &PathBuf, sid: &Uuid) -> Result<Self, String> {
        let json = std::fs::read_to_string(Self::state_path(workspace, sid))
            .map_err(|e| format!("read {sid}: {e}"))?;
        serde_json::from_str(&json).map_err(|e| format!("parse: {e}"))
    }

    pub fn list_sessions(workspace: &PathBuf) -> Result<Vec<SessionSummary>, String> {
        let dir = Self::state_dir(workspace);
        if !dir.exists() { return Ok(vec![]); }
        let mut out = vec![];
        for e in std::fs::read_dir(&dir).map_err(|e| format!("dir: {e}"))? {
            let e = e.map_err(|e| format!("entry: {e}"))?;
            if let Ok(json) = std::fs::read_to_string(e.path()) {
                if let Ok(s) = serde_json::from_str::<LocalSessionState>(&json) {
                    out.push(SessionSummary { session_id: s.session_id, goal: s.goal,
                        state: s.state, current_phase: s.current_phase,
                        created_at: s.created_at, updated_at: s.updated_at });
                }
            }
        }
        out.sort_by_key(|s| s.updated_at); out.reverse();
        Ok(out)
    }

    pub fn find_latest(workspace: &PathBuf) -> Result<Option<Self>, String> {
        if let Some(s) = Self::list_sessions(workspace)?.first() {
            Self::load(workspace, &s.session_id).map(Some)
        } else { Ok(None) }
    }

    pub fn add_phase_result(&mut self, r: PhaseResult) {
        self.phase_results.push(r); self.updated_at = chrono::Utc::now();
    }

    pub fn set_gate(&mut self, gate: &ConfirmationGate) {
        self.current_phase = gate.phase;
        self.pending_gate = Some(PendingGate { gate_id: gate.gate_id, phase: gate.phase,
            artifact_summary: gate.artifact_summary.clone().unwrap_or_default(),
            created_at: gate.created_at });
        self.updated_at = chrono::Utc::now();
    }

    pub fn clear_gate(&mut self) { self.pending_gate = None; self.updated_at = chrono::Utc::now(); }

    pub fn advance_phase(&mut self, next: SessionPhase) { self.current_phase = next; self.updated_at = chrono::Utc::now(); }

    pub fn complete(&mut self) {
        self.state = SessionState::Completed; self.current_phase = SessionPhase::Complete;
        self.updated_at = chrono::Utc::now();
    }

    pub fn abort(&mut self) { self.state = SessionState::Aborted; self.updated_at = chrono::Utc::now(); }

    pub fn is_active(&self) -> bool { matches!(self.state, SessionState::Active) }

    pub fn is_at_gate(&self) -> bool { self.pending_gate.is_some() }
}

use std::path::PathBuf;
use crate::session_state::LocalSessionState;
use crate::gate_handler::{GateHandler, GateResult};
use openforce_domain::session_phase::{SessionPhase, ConfirmationGate};

pub struct SessionRepl {
    pub session: LocalSessionState,
    pub workspace: PathBuf,
    pub interactive: bool,
}

impl SessionRepl {
    pub fn new(session: LocalSessionState, workspace: PathBuf, interactive: bool) -> Self {
        Self { session, workspace, interactive }
    }

    pub async fn run(&mut self) -> Result<(), String> {
        loop {
            let next = match self.session.current_phase.next_phase() {
                Some(p) => p,
                None => { println!("Session complete."); break; }
            };

            if next.is_gate() {
                self.session.advance_phase(next);
                let gate = ConfirmationGate::new(
                    self.session.session_id, next,
                    format!("Phase {} completed", self.session.current_phase.as_str()),
                    self.session.plan_epoch,
                );
                self.session.set_gate(&gate);
                self.session.save()?;

                let result = GateHandler::handle(&gate, 1, 1, self.interactive).await;
                match result {
                    GateResult::Approve => {
                        self.session.clear_gate();
                        if let Some(after) = next.next_phase() { self.session.advance_phase(after); }
                        self.session.save()?;
                        println!("Gate approved → {}", next.next_phase().map(|p| p.as_str().to_string()).unwrap_or_default());
                    }
                    GateResult::Reject(fb) => {
                        self.session.clear_gate();
                        self.session.plan_epoch += 1;
                        self.session.last_summary = Some(fb);
                        self.session.save()?;
                        println!("Rejected. Epoch {} — replan.", self.session.plan_epoch);
                    }
                    GateResult::Cancel => { self.session.abort(); self.session.save()?; break; }
                }
            } else {
                self.session.advance_phase(next);
                self.session.save()?;
                println!("Phase → {} | continue with: openforce continue", next.as_str());
                if next.is_terminal() { self.session.complete(); self.session.save()?; break; }
            }
        }
        Ok(())
    }
}

use openforce_domain::session_phase::{SessionPhase, ConfirmationGate};

pub enum GateResult { Approve, Reject(String), Cancel }

pub struct GateHandler;

impl GateHandler {
    pub async fn handle(gate: &ConfirmationGate, ok: usize, total: usize, interactive: bool) -> GateResult {
        Self::print_summary(gate, ok, total);
        if interactive { Self::prompt().await } else { println!("  openforce approve/continue | reject \"feedback\""); GateResult::Approve }
    }

    fn print_summary(gate: &ConfirmationGate, ok: usize, total: usize) {
        let name = match gate.phase { SessionPhase::ConfirmDesign => "Design", SessionPhase::ConfirmDev => "Development", SessionPhase::ConfirmFinal => "Final", _ => "" };
        println!("\n  [Gate: {name}] {}/{} tasks OK.", ok, total);
    }

    async fn prompt() -> GateResult {
        use tokio::io::AsyncBufReadExt;
        let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
        loop {
            print!("> "); let _ = std::io::Write::flush(&mut std::io::stdout());
            if let Ok(Some(line)) = lines.next_line().await {
                let l = line.trim().to_lowercase();
                if matches!(l.as_str(), "approve" | "yes" | "y") { return GateResult::Approve; }
                if l.starts_with("reject ") { return GateResult::Reject(line[7..].into()); }
                if matches!(l.as_str(), "cancel" | "quit") { return GateResult::Cancel; }
                println!("  approve | reject <reason> | cancel");
            } else { return GateResult::Cancel; }
        }
    }
}

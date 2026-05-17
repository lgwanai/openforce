use std::path::PathBuf;
use uuid::Uuid;

use crate::session_state::LocalSessionState;

enum Backend {
    Redis(openforce_redis_session::store::RedisSessionStore),
    Local,
}

pub struct SessionManager {
    pub workspace: PathBuf,
    backend: Backend,
}

impl SessionManager {
    pub async fn new(workspace: PathBuf) -> Self {
        let redis_url = std::env::var("REDIS_URL").unwrap_or_default();
        let backend = if !redis_url.is_empty() {
            match openforce_redis_session::store::RedisSessionStore::connect(&redis_url).await {
                Ok(store) => { tracing::info!("Session store: Redis"); Backend::Redis(store) }
                Err(e) => { tracing::warn!("Redis unavailable ({}), fallback to local", e); Backend::Local }
            }
        } else { tracing::info!("Session store: local (.openforce/sessions/)"); Backend::Local };
        Self { workspace, backend }
    }

    pub async fn create(&mut self, goal: &str) -> Result<LocalSessionState, String> {
        match &mut self.backend {
            Backend::Redis(redis) => {
                let r = redis.create(&self.workspace.display().to_string(), goal).await?;
                Ok(self.to_local(&r))
            }
            Backend::Local => {
                let s = LocalSessionState::create(goal.into(), self.workspace.clone());
                s.save()?;
                println!("Session {} created: {goal}", s.session_id);
                Ok(s)
            }
        }
    }

    pub async fn resume(&mut self, id: Option<&str>) -> Result<LocalSessionState, String> {
        match &mut self.backend {
            Backend::Redis(redis) => {
                let r = match id {
                    Some(sid) => redis.load(&Uuid::parse_str(sid).map_err(|e| format!("invalid: {e}"))?).await?
                        .ok_or("session not found")?,
                    None => {
                        let list = redis.list(Some(&self.workspace.display().to_string())).await?;
                        let first = list.first().ok_or("no sessions")?;
                        redis.load(&first.session_id).await?.ok_or("session not found")?
                    }
                };
                Ok(self.to_local(&r))
            }
            Backend::Local => self.resume_local(id),
        }
    }

    pub async fn list(&mut self) -> Result<(), String> {
        match &mut self.backend {
            Backend::Redis(redis) => {
                let sessions = redis.list(Some(&self.workspace.display().to_string())).await?;
                if sessions.is_empty() { println!("No sessions."); return Ok(()); }
                for s in &sessions {
                    println!("  {:>8}  [{:>16}]  {}  {}",
                        &s.session_id.to_string()[..8], s.current_phase,
                        s.updated_at.format("%m-%d %H:%M"), s.goal);
                }
            }
            Backend::Local => {
                let sessions = LocalSessionState::list_sessions(&self.workspace)?;
                if sessions.is_empty() { println!("No sessions."); return Ok(()); }
                for s in &sessions {
                    println!("  {:>8}  [{:>16}]  {}  {}",
                        &s.session_id.to_string()[..8], s.current_phase.as_str(),
                        s.updated_at.format("%m-%d %H:%M"), s.goal);
                }
            }
        }
        Ok(())
    }

    pub async fn cancel(&mut self, id: &str) -> Result<(), String> {
        let sid = Uuid::parse_str(id).map_err(|e| format!("invalid: {e}"))?;
        match &mut self.backend {
            Backend::Redis(redis) => redis.cancel(&sid).await,
            Backend::Local => {
                let mut s = LocalSessionState::load(&self.workspace, &sid)?;
                s.abort(); s.save()?;
                println!("Session {id} cancelled."); Ok(())
            }
        }
    }

    pub async fn approve(&mut self, id: Option<&str>) -> Result<LocalSessionState, String> {
        match &mut self.backend {
            Backend::Redis(redis) => {
                let s = match id {
                    Some(sid) => redis.load(&Uuid::parse_str(sid).map_err(|e| format!("invalid: {e}"))?).await?
                        .ok_or("session not found")?,
                    None => {
                        let list = redis.list(Some(&self.workspace.display().to_string())).await?;
                        redis.load(&list.first().ok_or("no sessions")?.session_id).await?.ok_or("session not found")?
                    }
                };
                let gid = s.pending_gate_id.ok_or("no pending gate")?;
                redis.resolve_gate(&gid, true, None).await?;
                println!("Gate approved. Phase → {}", s.current_phase.next_phase().map(|p| p.as_str().to_string()).unwrap_or_default());
                let r = redis.load(&s.session_id).await?.ok_or("session lost")?;
                Ok(self.to_local(&r))
            }
            Backend::Local => self.approve_local(id),
        }
    }

    pub async fn reject(&mut self, id: Option<&str>, feedback: &str) -> Result<LocalSessionState, String> {
        match &mut self.backend {
            Backend::Redis(redis) => {
                let s = match id {
                    Some(sid) => redis.load(&Uuid::parse_str(sid).map_err(|e| format!("invalid: {e}"))?).await?
                        .ok_or("session not found")?,
                    None => {
                        let list = redis.list(Some(&self.workspace.display().to_string())).await?;
                        redis.load(&list.first().ok_or("no sessions")?.session_id).await?.ok_or("session not found")?
                    }
                };
                let gid = s.pending_gate_id.ok_or("no pending gate")?;
                redis.resolve_gate(&gid, false, Some(feedback)).await?;
                println!("Gate rejected. Epoch {} — replanning.", s.plan_epoch + 1);
                let r = redis.load(&s.session_id).await?.ok_or("session lost")?;
                Ok(self.to_local(&r))
            }
            Backend::Local => self.reject_local(id, feedback),
        }
    }

    pub async fn save(&mut self, s: &LocalSessionState) -> Result<(), String> {
        match &mut self.backend {
            Backend::Redis(redis) => {
                let r = openforce_redis_session::store::RedisSessionState {
                    session_id: s.session_id, goal: s.goal.clone(),
                    state: s.state, current_phase: s.current_phase,
                    plan_version: s.plan_version, plan_epoch: s.plan_epoch,
                    workspace: self.workspace.display().to_string(),
                    pending_gate_id: s.pending_gate.as_ref().map(|g| g.gate_id),
                    created_at: s.created_at, updated_at: s.updated_at,
                };
                redis.save(&r).await
            }
            Backend::Local => s.save(),
        }
    }

    // ── Local fallback methods ──

    fn resume_local(&self, id: Option<&str>) -> Result<LocalSessionState, String> {
        match id {
            Some(sid) => {
                let sid = Uuid::parse_str(sid).map_err(|e| format!("invalid: {e}"))?;
                LocalSessionState::load(&self.workspace, &sid)
            }
            None => LocalSessionState::find_latest(&self.workspace)?
                .ok_or_else(|| "no sessions found".into()),
        }
    }

    fn approve_local(&self, id: Option<&str>) -> Result<LocalSessionState, String> {
        let mut s = self.resume_local(id)?;
        if !s.is_at_gate() { return Err("no pending gate".into()); }
        let next = s.current_phase.next_phase().ok_or("session complete")?;
        s.clear_gate(); s.advance_phase(next); s.save()?;
        println!("Gate approved. Phase → {}", next.as_str());
        Ok(s)
    }

    fn reject_local(&self, id: Option<&str>, feedback: &str) -> Result<LocalSessionState, String> {
        let mut s = self.resume_local(id)?;
        if !s.is_at_gate() { return Err("no pending gate".into()); }
        s.clear_gate(); s.plan_epoch += 1;
        s.last_summary = Some(format!("Rejected: {feedback}"));
        s.save()?;
        println!("Gate rejected. Epoch {} — replanning.", s.plan_epoch);
        Ok(s)
    }

    fn to_local(&self, r: &openforce_redis_session::store::RedisSessionState) -> LocalSessionState {
        LocalSessionState {
            session_id: r.session_id, goal: r.goal.clone(),
            state: r.state, current_phase: r.current_phase,
            plan_version: r.plan_version, plan_epoch: r.plan_epoch,
            workspace: self.workspace.clone(),
            pending_gate: None, phase_results: vec![], last_summary: None,
            created_at: r.created_at, updated_at: r.updated_at,
        }
    }
}

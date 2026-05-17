use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands, Client};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use tracing::info;
use openforce_domain::session::SessionState;
use openforce_domain::session_phase::{SessionPhase, ConfirmationGate, GateStatus};

pub struct RedisSessionStore {
    conn: MultiplexedConnection,
    ttl_seconds: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisSessionState {
    pub session_id: Uuid, pub goal: String, pub state: SessionState,
    pub current_phase: SessionPhase, pub plan_version: i32, pub plan_epoch: i32,
    pub workspace: String, pub pending_gate_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>, pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseResultEntry {
    pub phase: String, pub tasks_total: usize, pub tasks_ok: usize,
    pub plan_epoch: i32, pub summary: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: Uuid, pub goal: String, pub state: String,
    pub current_phase: String, pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl RedisSessionStore {
    pub async fn connect(url: &str) -> Result<Self, String> {
        let client = Client::open(url).map_err(|e| format!("redis: {e}"))?;
        let conn = client.get_multiplexed_async_connection().await
            .map_err(|e| format!("redis: {e}"))?;
        info!("Redis session store connected");
        Ok(Self { conn, ttl_seconds: 86400 })
    }

    pub fn with_ttl(mut self, t: usize) -> Self { self.ttl_seconds = t; self }

    fn sk(id: &Uuid) -> String { format!("session:{id}") }
    fn rk(id: &Uuid) -> String { format!("session:{id}:results") }
    fn gk(id: &Uuid) -> String { format!("gate:{id}") }
    const AK: &'static str = "sessions:active";

    // ── CRUD ──

    pub async fn create(&mut self, ws: &str, goal: &str) -> Result<RedisSessionState, String> {
        let id = Uuid::now_v7(); let now = chrono::Utc::now();
        let s = RedisSessionState { session_id: id, goal: goal.into(), state: SessionState::Active,
            current_phase: SessionPhase::Understand, plan_version: 0, plan_epoch: 1,
            workspace: ws.into(), pending_gate_id: None, created_at: now, updated_at: now };
        self.write(&s).await?;
        let _ =self.conn.sadd(Self::AK, id.to_string()).await.map_err(|e| format!("sadd: {e}"))?;
        self.ttl(&id).await?;
        Ok(s)
    }

    pub async fn load(&mut self, id: &Uuid) -> Result<Option<RedisSessionState>, String> {
        let f: Vec<(String, String)> = self.conn.hgetall(Self::sk(id)).await.map_err(|e| format!("hgetall: {e}"))?;
        if f.is_empty() { return Ok(None); }
        Self::to_state(id, &f)
    }

    pub async fn save(&mut self, s: &RedisSessionState) -> Result<(), String> {
        self.write(s).await?; self.ttl(&s.session_id).await
    }

    pub async fn list(&mut self, ws: Option<&str>) -> Result<Vec<SessionSummary>, String> {
        let ids: Vec<String> = self.conn.smembers(Self::AK).await.map_err(|e| format!("smembers: {e}"))?;
        let mut out = vec![];
        for s in ids.iter().filter_map(|i| Uuid::parse_str(i).ok()) {
            if let Ok(Some(r)) = self.load(&s).await {
                if ws.map_or(true, |w| r.workspace == w) {
                    out.push(SessionSummary { session_id: r.session_id, goal: r.goal,
                        state: r.state.as_str().into(), current_phase: r.current_phase.as_str().into(),
                        updated_at: r.updated_at });
                }
            }
        }
        out.sort_by_key(|s| s.updated_at); out.reverse();
        Ok(out)
    }

    pub async fn complete(&mut self, id: &Uuid) -> Result<(), String> {
        if let Some(mut s) = self.load(id).await? {
            s.state = SessionState::Completed; s.current_phase = SessionPhase::Complete;
            s.updated_at = chrono::Utc::now(); self.write(&s).await?;
            let _: bool = self.conn.srem(Self::AK, id.to_string()).await.map_err(|e| format!("{e}")).unwrap_or(false);
        }
        Ok(())
    }

    pub async fn cancel(&mut self, id: &Uuid) -> Result<(), String> {
        if let Some(mut s) = self.load(id).await? {
            s.state = SessionState::Aborted; s.updated_at = chrono::Utc::now();
            self.write(&s).await?;
            let _: bool = self.conn.srem(Self::AK, id.to_string()).await.map_err(|e| format!("{e}")).unwrap_or(false);
        }
        Ok(())
    }

    // ── Phase ──

    pub async fn advance_phase(&mut self, id: &Uuid, next: SessionPhase) -> Result<(), String> {
        if let Some(mut s) = self.load(id).await? {
            s.current_phase = next; s.updated_at = chrono::Utc::now();
            self.write(&s).await?;
        }
        Ok(())
    }

    // ── Gate ──

    pub async fn create_gate(
        &mut self, sid: Uuid, phase: SessionPhase,
        summary: &str, plan_epoch: i32,
    ) -> Result<ConfirmationGate, String> {
        let gate = ConfirmationGate::new(sid, phase, summary.into(), plan_epoch);
        let f: Vec<(&str, String)> = vec![
            ("session_id", sid.to_string()), ("phase", phase.as_str().into()),
            ("status", "pending".into()), ("artifact_summary", summary.into()),
            ("plan_epoch", plan_epoch.to_string()), ("created_at", gate.created_at.to_rfc3339()),
        ];
        let _ =self.conn.hset_multiple(Self::gk(&gate.gate_id), &f).await.map_err(|e| format!("hset: {e}"))?;
        if let Some(mut s) = self.load(&sid).await? {
            s.pending_gate_id = Some(gate.gate_id); s.updated_at = chrono::Utc::now();
            self.write(&s).await?;
        }
        let _: i64 = self.conn.publish("swarmos:gates", format!("created:{}", gate.gate_id)).await.map_err(|e| format!("{e}")).unwrap_or(0);
        Ok(gate)
    }

    pub async fn resolve_gate(
        &mut self, gid: &Uuid, approved: bool, feedback: Option<&str>,
    ) -> Result<ConfirmationGate, String> {
        let f: Vec<(String, String)> = self.conn.hgetall(Self::gk(gid)).await.map_err(|e| format!("hgetall: {e}"))?;
        if f.is_empty() { return Err("gate not found".into()); }
        let v = |k: &str| f.iter().find(|(x,_)| x==k).map(|(_,v)| v.clone());
        let sid: Uuid = v("session_id").and_then(|x| Uuid::parse_str(&x).ok()).ok_or("bad sid")?;
        let phase = v("phase").and_then(|x| SessionPhase::from_str(&x)).unwrap_or_default();
        let mut gate = ConfirmationGate { gate_id: *gid, session_id: sid, phase,
            status: GateStatus::Pending, user_feedback: None,
            artifact_summary: v("artifact_summary"),
            plan_epoch: v("plan_epoch").and_then(|x| x.parse().ok()).unwrap_or(0),
            created_at: v("created_at").and_then(|x| chrono::DateTime::parse_from_rfc3339(&x).ok().map(|t| t.with_timezone(&chrono::Utc))).unwrap_or_else(chrono::Utc::now),
            resolved_at: None };

        if approved { gate.approve(); } else { gate.reject(feedback.unwrap_or("").into()); }
        let _: bool = self.conn.hset(Self::gk(gid), "status", gate.status.as_str()).await.map_err(|e| format!("{e}")).unwrap_or(false);
        let _: bool = self.conn.hset(Self::gk(gid), "resolved_at", chrono::Utc::now().to_rfc3339()).await.map_err(|e| format!("{e}")).unwrap_or(false);
        if let Some(fb) = &gate.user_feedback { let _: bool = self.conn.hset(Self::gk(gid), "user_feedback", fb.as_str()).await.map_err(|e| format!("{e}")).unwrap_or(false); }

        if let Some(mut s) = self.load(&sid).await? {
            s.pending_gate_id = None;
            if approved { if let Some(next) = phase.next_phase() { s.current_phase = next; } }
            else { s.plan_epoch += 1; }
            s.updated_at = chrono::Utc::now(); self.write(&s).await?;
        }
        let ev = if approved { "approved" } else { "rejected" };
        let _: i64 = self.conn.publish("swarmos:gates", format!("{ev}:{gid}")).await.map_err(|e| format!("{e}")).unwrap_or(0);
        Ok(gate)
    }

    pub async fn get_gate(&mut self, sid: &Uuid) -> Result<Option<ConfirmationGate>, String> {
        let gid = match self.load(sid).await?.and_then(|s| s.pending_gate_id) { Some(id) => id, None => return Ok(None) };
        let f: Vec<(String, String)> = self.conn.hgetall(Self::gk(&gid)).await.map_err(|e| format!("hgetall: {e}"))?;
        if f.is_empty() { return Ok(None); }
        let v = |k: &str| f.iter().find(|(x,_)| x==k).map(|(_,v)| v.clone());
        Ok(Some(ConfirmationGate { gate_id: gid, session_id: *sid,
            phase: v("phase").and_then(|x| SessionPhase::from_str(&x)).unwrap_or_default(),
            status: v("status").and_then(|x| match x.as_str() { "approved" => Some(GateStatus::Approved), "rejected" => Some(GateStatus::Rejected), _ => Some(GateStatus::Pending) }).unwrap_or(GateStatus::Pending),
            user_feedback: v("user_feedback"),
            artifact_summary: v("artifact_summary"),
            plan_epoch: v("plan_epoch").and_then(|x| x.parse().ok()).unwrap_or(0),
            created_at: v("created_at").and_then(|x| chrono::DateTime::parse_from_rfc3339(&x).ok().map(|t| t.with_timezone(&chrono::Utc))).unwrap_or_else(chrono::Utc::now),
            resolved_at: v("resolved_at").and_then(|x| chrono::DateTime::parse_from_rfc3339(&x).ok().map(|t| t.with_timezone(&chrono::Utc))),
        }))
    }

    // ── Results ──

    pub async fn add_result(&mut self, sid: &Uuid, r: &PhaseResultEntry) -> Result<(), String> {
        let j = serde_json::to_string(r).map_err(|e| format!("json: {e}"))?;
        let _ =self.conn.rpush(Self::rk(sid), j).await.map_err(|e| format!("rpush: {e}"))?;
        Ok(())
    }

    // ── Internal ──

    async fn write(&mut self, s: &RedisSessionState) -> Result<(), String> {
        let f: Vec<(&str, String)> = vec![
            ("goal", s.goal.clone()), ("state", s.state.as_str().into()),
            ("current_phase", s.current_phase.as_str().into()),
            ("plan_version", s.plan_version.to_string()), ("plan_epoch", s.plan_epoch.to_string()),
            ("workspace", s.workspace.clone()),
            ("pending_gate_id", s.pending_gate_id.map(|i| i.to_string()).unwrap_or_default()),
            ("created_at", s.created_at.to_rfc3339()), ("updated_at", chrono::Utc::now().to_rfc3339()),
        ];
        let _ =self.conn.hset_multiple(Self::sk(&s.session_id), &f).await.map_err(|e| format!("hset: {e}"))?;
        Ok(())
    }

    async fn ttl(&mut self, id: &Uuid) -> Result<(), String> {
        let _: bool = self.conn.expire(Self::sk(id), self.ttl_seconds as i64).await.map_err(|e| format!("{e}")).unwrap_or(false);
        Ok(())
    }

    fn to_state(id: &Uuid, f: &[(String, String)]) -> Result<Option<RedisSessionState>, String> {
        let v = |k: &str| f.iter().find(|(x,_)| x==k).map(|(_,v)| v.clone());
        if v("goal").unwrap_or_default().is_empty() { return Ok(None); }
        Ok(Some(RedisSessionState {
            session_id: *id, goal: v("goal").unwrap_or_default(),
            state: v("state").and_then(|x| SessionState::from_str(&x)).unwrap_or(SessionState::Active),
            current_phase: v("current_phase").and_then(|x| SessionPhase::from_str(&x)).unwrap_or_default(),
            plan_version: v("plan_version").and_then(|x| x.parse().ok()).unwrap_or(0),
            plan_epoch: v("plan_epoch").and_then(|x| x.parse().ok()).unwrap_or(1),
            workspace: v("workspace").unwrap_or_default(),
            pending_gate_id: v("pending_gate_id").and_then(|x| if x.is_empty() { None } else { Uuid::parse_str(&x).ok() }),
            created_at: v("created_at").and_then(|x| chrono::DateTime::parse_from_rfc3339(&x).ok().map(|t| t.with_timezone(&chrono::Utc))).unwrap_or_else(chrono::Utc::now),
            updated_at: v("updated_at").and_then(|x| chrono::DateTime::parse_from_rfc3339(&x).ok().map(|t| t.with_timezone(&chrono::Utc))).unwrap_or_else(chrono::Utc::now),
        }))
    }
}

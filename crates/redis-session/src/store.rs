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
pub struct WorkerSubTask {
    pub id: usize, pub description: String, pub status: String, pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSnapshot {
    pub worker_id: String, pub role: String, pub task: String,
    pub subtasks: Vec<WorkerSubTask>,
    pub acceptance_criteria: Vec<String>,
    pub progress: String, pub inputs: Vec<String>, pub outputs: Vec<String>,
    pub intermediate_artifacts: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub artifact_id: String, pub artifact_type: String,
    pub location: String, pub created_by: String,
    pub metadata: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
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

    // ── Planner Snapshot ──

    pub async fn set_planner(&mut self, sid: &Uuid, classification: &str, decomposition: &str, roles: &[String]) -> Result<(), String> {
        let key = format!("session:{sid}:planner");
        let f: Vec<(&str, String)> = vec![
            ("classification", classification.into()), ("decomposition", decomposition.into()),
            ("roles", roles.join(",")), ("epoch", chrono::Utc::now().to_rfc3339()),
        ];
        let _ =self.conn.hset_multiple(&key, &f).await.map_err(|e| format!("hset: {e}"))?;
        Ok(())
    }

    // ── Worker Snapshot ──

    pub async fn set_worker_snapshot(&mut self, sid: &Uuid, wid: &str, sn: &WorkerSnapshot) -> Result<(), String> {
        let key = format!("session:{sid}:worker:{wid}");
        let j = serde_json::to_string(sn).map_err(|e| format!("json: {e}"))?;
        let _: bool = self.conn.hset(&key, "snapshot", j).await.map_err(|e| format!("{e}")).unwrap_or(false);
        Ok(())
    }

    pub async fn get_worker_snapshot(&mut self, sid: &Uuid, wid: &str) -> Result<Option<WorkerSnapshot>, String> {
        let key = format!("session:{sid}:worker:{wid}");
        let j: Option<String> = self.conn.hget(&key, "snapshot").await.map_err(|e| format!("hget: {e}"))?;
        j.map(|x| serde_json::from_str(&x).map_err(|e| format!("parse: {e}"))).transpose()
    }

    pub async fn list_workers(&mut self, sid: &Uuid) -> Result<Vec<String>, String> {
        let pattern = format!("session:{sid}:worker:*");
        let keys: Vec<String> = self.conn.keys(&pattern).await.map_err(|e| format!("keys: {e}"))?;
        Ok(keys.iter().filter_map(|k| k.rsplit(':').next().map(String::from)).collect())
    }

    // ── Artifact Registry ──

    pub async fn add_artifact(&mut self, sid: &Uuid, a: &ArtifactRef) -> Result<(), String> {
        let key = format!("session:{sid}:artifacts");
        let j = serde_json::to_string(a).map_err(|e| format!("json: {e}"))?;
        let _ =self.conn.rpush(&key, j).await.map_err(|e| format!("rpush: {e}"))?;
        Ok(())
    }

    pub async fn list_artifacts(&mut self, sid: &Uuid) -> Result<Vec<ArtifactRef>, String> {
        let key = format!("session:{sid}:artifacts");
        let js: Vec<String> = self.conn.lrange(&key, 0, -1).await.map_err(|e| format!("lrange: {e}"))?;
        js.iter().map(|j| serde_json::from_str(j).map_err(|e| format!("parse: {e}"))).collect()
    }

    // ── Session Map (Worker context injection) ──

    pub async fn get_session_map(&mut self, sid: &Uuid, wid: &str) -> Result<String, String> {
        let state = self.load(sid).await?.ok_or("session not found")?;
        let short = sid.to_string().chars().take(8).collect::<String>();
        let mut map = format!("SESSION {short}: goal={goal} phase={phase} epoch={epoch}\n",
            goal=state.goal, phase=state.current_phase.as_str(), epoch=state.plan_epoch);

        let pkey = format!("session:{sid}:planner");
        let p: Vec<(String, String)> = self.conn.hgetall(&pkey).await.unwrap_or_default();
        if !p.is_empty() {
            let v = |k: &str| p.iter().find(|(x,_)| x==k).map(|(_,v)| v.clone());
            map.push_str(&format!("  PLANNER: classification={} roles=[{}]\n    Query: get_planner_output()\n",
                v("classification").unwrap_or_default(), v("roles").unwrap_or_default()));
        }

        let ws = self.list_workers(sid).await.unwrap_or_default();
        let others: Vec<_> = ws.iter().filter(|w| *w != wid).collect();
        if !others.is_empty() {
            map.push_str(&format!("  WORKERS ({}):", others.len()));
            for w in &others {
                if let Ok(Some(sn)) = self.get_worker_snapshot(sid, w).await {
                    let d = sn.subtasks.iter().filter(|t| t.status=="done").count();
                    map.push_str(&format!("\n    {w}: {}/{} done → get_worker_output(\"{w}\")", d, sn.subtasks.len()));
                }
            }
            map.push('\n');
        }

        if let Ok(arts) = self.list_artifacts(sid).await {
            if !arts.is_empty() {
                map.push_str(&format!("  ARTIFACTS ({}):", arts.len()));
                for a in &arts { map.push_str(&format!("\n    {} ({}) → {}", a.artifact_id, a.artifact_type, a.location)); }
                map.push_str("\n    Query: get_artifact(id)\n");
            }
        }

        map.push_str(
            "  TOOLS: get_planner_output() | get_worker_output(id) | get_artifact(id) | get_instructions() | search_session(q)"
        );
        Ok(map)
    }

    // ── Instructions ──

    pub async fn add_instruction(&mut self, sid: &Uuid, text: &str) -> Result<(), String> {
        let key = format!("session:{sid}:instructions");
        let entry = format!("{}|{}", chrono::Utc::now().to_rfc3339(), text);
        let _ =self.conn.rpush(&key, entry).await.map_err(|e| format!("rpush: {e}"))?;
        Ok(())
    }

    pub async fn get_instructions(&mut self, sid: &Uuid) -> Result<Vec<String>, String> {
        let key = format!("session:{sid}:instructions");
        self.conn.lrange(&key, 0, -1).await.map_err(|e| format!("lrange: {e}"))
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

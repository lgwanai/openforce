use openforce_domain::session::SessionState;
use openforce_domain::session_phase::{ConfirmationGate, GateStatus, SessionPhase};
use openforce_domain::worker_folder::WorkerOutputFolder;
use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands, Client};
use serde::{Deserialize, Serialize};
use tracing::info;
use uuid::Uuid;

/// Who is making the session operation — used for access control.
#[derive(Debug, Clone)]
pub enum Caller {
    Worker { worker_id: String, session_id: Uuid },
    Planner { session_id: Uuid },
    Scheduler { session_id: Uuid },
}

impl Caller {
    pub fn id(&self) -> &str {
        match self {
            Self::Worker { worker_id, .. } => worker_id,
            Self::Planner { .. } => "planner",
            Self::Scheduler { .. } => "scheduler",
        }
    }
    pub fn session_id(&self) -> Uuid {
        match self {
            Self::Worker { session_id, .. }
            | Self::Planner { session_id }
            | Self::Scheduler { session_id } => *session_id,
        }
    }
    pub fn is_worker(&self) -> bool {
        matches!(self, Self::Worker { .. })
    }
    pub fn is_planner(&self) -> bool {
        matches!(self, Self::Planner { .. })
    }
}

pub struct RedisSessionStore {
    conn: MultiplexedConnection,
    ttl_seconds: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisSessionState {
    pub session_id: Uuid,
    pub goal: String,
    pub state: SessionState,
    pub current_phase: SessionPhase,
    pub plan_version: i32,
    pub plan_epoch: i32,
    pub workspace: String,
    pub pending_gate_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseResultEntry {
    pub phase: String,
    pub tasks_total: usize,
    pub tasks_ok: usize,
    pub plan_epoch: i32,
    pub summary: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSubTask {
    pub id: usize,
    pub description: String,
    pub status: String,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSnapshot {
    pub worker_id: String,
    pub role: String,
    pub task: String,
    pub subtasks: Vec<WorkerSubTask>,
    pub acceptance_criteria: Vec<String>,
    pub progress: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub intermediate_artifacts: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

// ── Todo Tracking ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTodoItem {
    pub id: usize,
    pub content: String,
    pub status: String, // pending | in_progress | completed | cancelled | failed
    pub priority: String,
    pub agent: String,
    pub worker_id: Option<String>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub output_ref: Option<String>,
    pub acceptance_criteria: Vec<String>,
    pub depends_on: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTodoList {
    pub session_id: String,
    pub items: Vec<SessionTodoItem>,
    pub total: usize,
    pub done: usize,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl SessionTodoItem {
    pub fn is_ready(&self, all: &[SessionTodoItem]) -> bool {
        self.status == "pending"
            && self
                .depends_on
                .iter()
                .all(|d| all.iter().any(|t| t.id == *d && t.status == "completed"))
    }
    pub fn is_active(&self) -> bool {
        self.status == "in_progress"
    }
    pub fn is_done(&self) -> bool {
        self.status == "completed"
    }
}

impl SessionTodoList {
    pub fn from_task_tree(sid: &str, tasks: &[(String, String, String, Vec<String>)]) -> Self {
        let items: Vec<SessionTodoItem> = tasks
            .iter()
            .enumerate()
            .map(|(i, (role, title, desc, deps))| {
                let dep_ids: Vec<usize> = deps
                    .iter()
                    .filter_map(|d| {
                        tasks
                            .iter()
                            .position(|(_, t, _, _)| t == d)
                            .map(|idx| idx + 1)
                    })
                    .collect();
                SessionTodoItem {
                    id: i + 1,
                    content: format!("[{role}] {title}: {desc}"),
                    status: "pending".into(),
                    priority: "medium".into(),
                    agent: role.clone(),
                    worker_id: None,
                    started_at: None,
                    completed_at: None,
                    output_ref: None,
                    acceptance_criteria: vec![],
                    depends_on: dep_ids,
                }
            })
            .collect();
        let t = items.len();
        Self {
            session_id: sid.into(),
            items,
            total: t,
            done: 0,
            updated_at: chrono::Utc::now(),
        }
    }

    pub fn mark_in_progress(&mut self, id: usize, wid: &str) -> bool {
        if let Some(item) = self.items.iter_mut().find(|t| t.id == id) {
            item.status = "in_progress".into();
            item.worker_id = Some(wid.into());
            item.started_at = Some(chrono::Utc::now());
            self.updated_at = chrono::Utc::now();
            return true;
        }
        false
    }

    pub fn mark_done(&mut self, id: usize, out: Option<&str>) -> bool {
        if let Some(item) = self.items.iter_mut().find(|t| t.id == id) {
            item.status = "completed".into();
            item.completed_at = Some(chrono::Utc::now());
            item.output_ref = out.map(String::from);
            self.done = self.items.iter().filter(|t| t.is_done()).count();
            self.updated_at = chrono::Utc::now();
            return true;
        }
        false
    }

    pub fn mark_failed(&mut self, id: usize) {
        if let Some(item) = self.items.iter_mut().find(|t| t.id == id) {
            item.status = "failed".into();
            self.updated_at = chrono::Utc::now();
        }
    }

    pub fn next_pending(&self) -> Option<&SessionTodoItem> {
        self.items
            .iter()
            .filter(|t| t.status == "pending")
            .filter(|t| t.is_ready(&self.items))
            .min_by_key(|t| match t.priority.as_str() {
                "high" => 0u8,
                "medium" => 1,
                "low" => 2,
                _ => 1,
            })
    }

    pub fn progress_str(&self) -> String {
        let d = self.items.iter().filter(|t| t.is_done()).count();
        let a = self.items.iter().filter(|t| t.is_active()).count();
        format!("{}/{} ({} active)", d, self.total, a)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub artifact_id: String,
    pub artifact_type: String,
    pub location: String,
    pub created_by: String,
    pub metadata: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: Uuid,
    pub goal: String,
    pub state: String,
    pub current_phase: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl RedisSessionStore {
    pub async fn connect(url: &str) -> Result<Self, String> {
        let client = Client::open(url).map_err(|e| format!("redis: {e}"))?;
        let conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| format!("redis: {e}"))?;
        info!("Redis session store connected");
        Ok(Self {
            conn,
            ttl_seconds: std::env::var("REDIS_SESSION_TTL")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(86400),
        })
    }

    pub fn with_ttl(mut self, t: usize) -> Self {
        self.ttl_seconds = t;
        self
    }

    fn sk(id: &Uuid) -> String {
        format!("session:{id}")
    }
    fn rk(id: &Uuid) -> String {
        format!("session:{id}:results")
    }
    fn tk(id: &Uuid) -> String {
        format!("session:{id}:todos")
    }
    #[allow(dead_code)]
    fn wk(sid: &Uuid, wid: &str) -> String {
        format!("session:{sid}:worker:{wid}")
    }
    fn gk(id: &Uuid) -> String {
        format!("gate:{id}")
    }
    const AK: &'static str = "sessions:active";

    // ── CRUD ──

    pub async fn create(&mut self, ws: &str, goal: &str) -> Result<RedisSessionState, String> {
        let id = Uuid::now_v7();
        let now = chrono::Utc::now();
        let s = RedisSessionState {
            session_id: id,
            goal: goal.into(),
            state: SessionState::Active,
            current_phase: SessionPhase::understand(),
            plan_version: 0,
            plan_epoch: 1,
            workspace: ws.into(),
            pending_gate_id: None,
            created_at: now,
            updated_at: now,
        };
        self.write(&s).await?;
        let _: () = self
            .conn
            .sadd(Self::AK, id.to_string())
            .await
            .map_err(|e| format!("sadd: {e}"))?;
        self.ttl(&id).await?;
        Ok(s)
    }

    pub async fn load(&mut self, id: &Uuid) -> Result<Option<RedisSessionState>, String> {
        let f: Vec<(String, String)> = self
            .conn
            .hgetall(Self::sk(id))
            .await
            .map_err(|e| format!("hgetall: {e}"))?;
        if f.is_empty() {
            return Ok(None);
        }
        Self::to_state(id, &f)
    }

    pub async fn save(&mut self, s: &RedisSessionState) -> Result<(), String> {
        self.write(s).await?;
        self.ttl(&s.session_id).await
    }

    pub async fn list(&mut self, ws: Option<&str>) -> Result<Vec<SessionSummary>, String> {
        let ids: Vec<String> = self
            .conn
            .smembers(Self::AK)
            .await
            .map_err(|e| format!("smembers: {e}"))?;
        let mut out = vec![];
        for s in ids.iter().filter_map(|i| Uuid::parse_str(i).ok()) {
            if let Ok(Some(r)) = self.load(&s).await {
                if ws.map_or(true, |w| r.workspace == w) {
                    out.push(SessionSummary {
                        session_id: r.session_id,
                        goal: r.goal,
                        state: r.state.as_str().into(),
                        current_phase: r.current_phase.as_str().into(),
                        updated_at: r.updated_at,
                    });
                }
            }
        }
        out.sort_by_key(|s| s.updated_at);
        out.reverse();
        Ok(out)
    }

    pub async fn complete(&mut self, id: &Uuid) -> Result<(), String> {
        if let Some(mut s) = self.load(id).await? {
            s.state = SessionState::Completed;
            s.current_phase = SessionPhase::complete();
            s.updated_at = chrono::Utc::now();
            self.write(&s).await?;
            let _: bool = self
                .conn
                .srem(Self::AK, id.to_string())
                .await
                .map_err(|e| format!("{e}"))
                .unwrap_or(false);
        }
        Ok(())
    }

    pub async fn cancel(&mut self, id: &Uuid) -> Result<(), String> {
        if let Some(mut s) = self.load(id).await? {
            s.state = SessionState::Aborted;
            s.updated_at = chrono::Utc::now();
            self.write(&s).await?;
            let _: bool = self
                .conn
                .srem(Self::AK, id.to_string())
                .await
                .map_err(|e| format!("{e}"))
                .unwrap_or(false);
        }
        Ok(())
    }

    // ── Phase ──

    pub async fn advance_phase(&mut self, id: &Uuid, next: SessionPhase) -> Result<(), String> {
        if let Some(mut s) = self.load(id).await? {
            s.current_phase = next;
            s.updated_at = chrono::Utc::now();
            self.write(&s).await?;
        }
        Ok(())
    }

    // ── Gate ──

    /// Only Planner can create confirmation gates.
    pub async fn create_gate(
        &mut self,
        caller: &Caller,
        phase: SessionPhase,
        summary: &str,
        plan_epoch: i32,
    ) -> Result<ConfirmationGate, String> {
        if !caller.is_planner() {
            return Err("access denied: only planner can create gates".into());
        }
        let sid = caller.session_id();
        let phase_str = phase.as_str().to_string();
        let gate = ConfirmationGate::new(sid, phase, summary.into(), plan_epoch);
        let f: Vec<(&str, String)> = vec![
            ("session_id", sid.to_string()),
            ("phase", phase_str),
            ("status", "pending".into()),
            ("artifact_summary", summary.into()),
            ("plan_epoch", plan_epoch.to_string()),
            ("created_at", gate.created_at.to_rfc3339()),
        ];
        let _: () = self
            .conn
            .hset_multiple(Self::gk(&gate.gate_id), &f)
            .await
            .map_err(|e| format!("hset: {e}"))?;
        if let Some(mut s) = self.load(&sid).await? {
            s.pending_gate_id = Some(gate.gate_id);
            s.updated_at = chrono::Utc::now();
            self.write(&s).await?;
        }
        let _: i64 = self
            .conn
            .publish("swarmos:gates", format!("created:{}", gate.gate_id))
            .await
            .map_err(|e| format!("{e}"))
            .unwrap_or(0);
        Ok(gate)
    }

    /// Only Planner can resolve gates.
    pub async fn resolve_gate(
        &mut self,
        caller: &Caller,
        gid: &Uuid,
        approved: bool,
        feedback: Option<&str>,
    ) -> Result<ConfirmationGate, String> {
        if !caller.is_planner() {
            return Err("access denied: only planner can resolve gates".into());
        }
        let f: Vec<(String, String)> = self
            .conn
            .hgetall(Self::gk(gid))
            .await
            .map_err(|e| format!("hgetall: {e}"))?;
        if f.is_empty() {
            return Err("gate not found".into());
        }
        let v = |k: &str| f.iter().find(|(x, _)| x == k).map(|(_, v)| v.clone());
        let sid: Uuid = v("session_id")
            .and_then(|x| Uuid::parse_str(&x).ok())
            .ok_or("bad sid")?;
        let phase = v("phase")
            .and_then(|x| SessionPhase::from_str(&x))
            .unwrap_or_default();
        let mut gate = ConfirmationGate {
            gate_id: *gid,
            session_id: sid,
            phase,
            status: GateStatus::Pending,
            user_feedback: None,
            artifact_summary: v("artifact_summary"),
            plan_epoch: v("plan_epoch").and_then(|x| x.parse().ok()).unwrap_or(0),
            created_at: v("created_at")
                .and_then(|x| {
                    chrono::DateTime::parse_from_rfc3339(&x)
                        .ok()
                        .map(|t| t.with_timezone(&chrono::Utc))
                })
                .unwrap_or_else(chrono::Utc::now),
            resolved_at: None,
        };

        if approved {
            gate.approve();
        } else {
            gate.reject(feedback.unwrap_or("").into());
        }
        let _: bool = self
            .conn
            .hset(Self::gk(gid), "status", gate.status.as_str())
            .await
            .map_err(|e| format!("{e}"))
            .unwrap_or(false);
        let _: bool = self
            .conn
            .hset(
                Self::gk(gid),
                "resolved_at",
                chrono::Utc::now().to_rfc3339(),
            )
            .await
            .map_err(|e| format!("{e}"))
            .unwrap_or(false);
        if let Some(fb) = &gate.user_feedback {
            let _: bool = self
                .conn
                .hset(Self::gk(gid), "user_feedback", fb.as_str())
                .await
                .map_err(|e| format!("{e}"))
                .unwrap_or(false);
        }

        if let Some(mut s) = self.load(&sid).await? {
            s.pending_gate_id = None;
            if approved {
                if let Some(next) = gate.phase.next_phase() {
                    s.current_phase = next;
                }
            } else {
                s.plan_epoch += 1;
            }
            s.updated_at = chrono::Utc::now();
            self.write(&s).await?;
        }
        let ev = if approved { "approved" } else { "rejected" };
        let _: i64 = self
            .conn
            .publish("swarmos:gates", format!("{ev}:{gid}"))
            .await
            .map_err(|e| format!("{e}"))
            .unwrap_or(0);
        Ok(gate)
    }

    pub async fn get_gate(&mut self, sid: &Uuid) -> Result<Option<ConfirmationGate>, String> {
        let gid = match self.load(sid).await?.and_then(|s| s.pending_gate_id) {
            Some(id) => id,
            None => return Ok(None),
        };
        let f: Vec<(String, String)> = self
            .conn
            .hgetall(Self::gk(&gid))
            .await
            .map_err(|e| format!("hgetall: {e}"))?;
        if f.is_empty() {
            return Ok(None);
        }
        let v = |k: &str| f.iter().find(|(x, _)| x == k).map(|(_, v)| v.clone());
        Ok(Some(ConfirmationGate {
            gate_id: gid,
            session_id: *sid,
            phase: v("phase")
                .and_then(|x| SessionPhase::from_str(&x))
                .unwrap_or_default(),
            status: v("status")
                .and_then(|x| match x.as_str() {
                    "approved" => Some(GateStatus::Approved),
                    "rejected" => Some(GateStatus::Rejected),
                    _ => Some(GateStatus::Pending),
                })
                .unwrap_or(GateStatus::Pending),
            user_feedback: v("user_feedback"),
            artifact_summary: v("artifact_summary"),
            plan_epoch: v("plan_epoch").and_then(|x| x.parse().ok()).unwrap_or(0),
            created_at: v("created_at")
                .and_then(|x| {
                    chrono::DateTime::parse_from_rfc3339(&x)
                        .ok()
                        .map(|t| t.with_timezone(&chrono::Utc))
                })
                .unwrap_or_else(chrono::Utc::now),
            resolved_at: v("resolved_at").and_then(|x| {
                chrono::DateTime::parse_from_rfc3339(&x)
                    .ok()
                    .map(|t| t.with_timezone(&chrono::Utc))
            }),
        }))
    }

    // ── Results ──

    pub async fn add_result(&mut self, sid: &Uuid, r: &PhaseResultEntry) -> Result<(), String> {
        let j = serde_json::to_string(r).map_err(|e| format!("json: {e}"))?;
        let _: () = self
            .conn
            .rpush(Self::rk(sid), j)
            .await
            .map_err(|e| format!("rpush: {e}"))?;
        Ok(())
    }

    // ── Worker Output Folders ──

    pub async fn add_folders(
        &mut self,
        sid: &Uuid,
        epoch: i32,
        folders: &[WorkerOutputFolder],
    ) -> Result<(), String> {
        let key = format!("session:{}:folders:{}", sid, epoch);
        let json = serde_json::to_string(folders).map_err(|e| format!("json: {e}"))?;
        let _: () = self
            .conn
            .set(&key, &json)
            .await
            .map_err(|e| format!("set folders: {e}"))?;
        self.ttl_key(&key).await?;
        Ok(())
    }

    pub async fn get_folders(
        &mut self,
        sid: &Uuid,
        epoch: i32,
    ) -> Result<Vec<WorkerOutputFolder>, String> {
        let key = format!("session:{}:folders:{}", sid, epoch);
        let data: Option<String> = self
            .conn
            .get(&key)
            .await
            .map_err(|e| format!("get folders: {e}"))?;
        match data {
            Some(json) => serde_json::from_str(&json).map_err(|e| format!("parse folders: {e}")),
            None => Ok(vec![]),
        }
    }

    // ── Planner Snapshot ──

    pub async fn set_planner(
        &mut self,
        sid: &Uuid,
        classification: &str,
        decomposition: &str,
        roles: &[String],
    ) -> Result<(), String> {
        let key = format!("session:{sid}:planner");
        let f: Vec<(&str, String)> = vec![
            ("classification", classification.into()),
            ("decomposition", decomposition.into()),
            ("roles", roles.join(",")),
            ("epoch", chrono::Utc::now().to_rfc3339()),
        ];
        let _: () = self
            .conn
            .hset_multiple(&key, &f)
            .await
            .map_err(|e| format!("hset: {e}"))?;
        Ok(())
    }

    // ── Worker Snapshot (ACL: worker writes own data, all can read) ──

    /// Worker updates its OWN snapshot. Fails if caller is not the worker itself.
    pub async fn set_worker_snapshot(
        &mut self,
        caller: &Caller,
        sn: &WorkerSnapshot,
    ) -> Result<(), String> {
        let session_id = caller.session_id();
        let key = format!("session:{session_id}:worker:{}", sn.worker_id);
        // Worker must write its own snapshot; Scheduler can write any
        if caller.is_worker() {
            if let Caller::Worker { worker_id, .. } = caller {
                if &sn.worker_id != worker_id {
                    return Err(format!(
                        "access denied: worker {worker_id} cannot write as {}",
                        sn.worker_id
                    ));
                }
            }
        }
        let j = serde_json::to_string(sn).map_err(|e| format!("json: {e}"))?;
        let _: bool = self
            .conn
            .hset(&key, "snapshot", &j)
            .await
            .map_err(|e| format!("hset worker snapshot: {e}"))?;
        Ok(())
    }

    /// Read any worker's snapshot (no restriction).
    pub async fn get_worker_snapshot(
        &mut self,
        sid: &Uuid,
        wid: &str,
    ) -> Result<Option<WorkerSnapshot>, String> {
        let key = format!("session:{sid}:worker:{wid}");
        let j: Option<String> = self
            .conn
            .hget(&key, "snapshot")
            .await
            .map_err(|e| format!("hget: {e}"))?;
        j.map(|x| serde_json::from_str(&x).map_err(|e| format!("parse: {e}")))
            .transpose()
    }

    pub async fn list_workers(&mut self, sid: &Uuid) -> Result<Vec<String>, String> {
        let pattern = format!("session:{sid}:worker:*");
        // Use SCAN instead of KEYS to avoid blocking Redis in production
        let mut keys = Vec::new();
        let mut cursor: u64 = 0;
        loop {
            let (new_cursor, batch): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&pattern)
                .arg("COUNT")
                .arg(100)
                .query_async(&mut self.conn)
                .await
                .map_err(|e| format!("scan: {e}"))?;
            keys.extend(batch);
            cursor = new_cursor;
            if cursor == 0 {
                break;
            }
        }
        Ok(keys
            .iter()
            .filter_map(|k| k.rsplit(':').next().map(String::from))
            .collect())
    }

    /// Planner/Scheduler: terminate a worker (sets status to "terminated").
    /// Workers cannot terminate themselves or others.
    // ── Todo List ──

    pub async fn set_todo_list(
        &mut self,
        sid: &Uuid,
        todos: &SessionTodoList,
    ) -> Result<(), String> {
        let json = serde_json::to_string(todos).map_err(|e| format!("json: {e}"))?;
        let _: () = self
            .conn
            .set(Self::tk(sid), &json)
            .await
            .map_err(|e| format!("set todos: {e}"))?;
        self.ttl_key(&Self::tk(sid)).await
    }

    pub async fn get_todo_list(&mut self, sid: &Uuid) -> Result<Option<SessionTodoList>, String> {
        let data: Option<String> = self
            .conn
            .get(Self::tk(sid))
            .await
            .map_err(|e| format!("get todos: {e}"))?;
        match data {
            Some(json) => serde_json::from_str(&json)
                .map(Some)
                .map_err(|e| format!("parse todos: {e}")),
            None => Ok(None),
        }
    }

    pub async fn update_todo_status(
        &mut self,
        sid: &Uuid,
        todo_id: usize,
        status: &str,
        wid: &str,
        output: Option<&str>,
    ) -> Result<(), String> {
        if let Some(mut todos) = self.get_todo_list(sid).await? {
            match status {
                "in_progress" => {
                    todos.mark_in_progress(todo_id, wid);
                }
                "completed" => {
                    todos.mark_done(todo_id, output);
                }
                "failed" => {
                    todos.mark_failed(todo_id);
                }
                _ => {}
            }
            self.set_todo_list(sid, &todos).await?;
        }
        Ok(())
    }

    pub async fn terminate_worker(
        &mut self,
        caller: &Caller,
        target_wid: &str,
    ) -> Result<(), String> {
        if caller.is_worker() {
            return Err("access denied: workers cannot terminate".into());
        }
        let sid = caller.session_id();
        let key = format!("session:{sid}:worker:{target_wid}");
        let _: bool = self
            .conn
            .hset(&key, "status", "terminated")
            .await
            .map_err(|e| format!("{e}"))
            .unwrap_or(false);
        let _: bool = self
            .conn
            .hset(&key, "terminated_by", caller.id())
            .await
            .map_err(|e| format!("{e}"))
            .unwrap_or(false);
        let _: bool = self
            .conn
            .hset(&key, "terminated_at", chrono::Utc::now().to_rfc3339())
            .await
            .map_err(|e| format!("{e}"))
            .unwrap_or(false);
        tracing::warn!("worker {target_wid} terminated by {}", caller.id());
        Ok(())
    }

    // ── Artifact Registry ──

    /// Only workers can add artifacts, and only under their own ID.
    pub async fn add_artifact(&mut self, caller: &Caller, a: &ArtifactRef) -> Result<(), String> {
        if !caller.is_worker() {
            return Err("access denied: only workers can add artifacts".into());
        }
        if let Caller::Worker {
            worker_id,
            session_id,
        } = caller
        {
            if &a.created_by != worker_id {
                return Err(format!(
                    "access denied: {worker_id} cannot create as {}",
                    a.created_by
                ));
            }
            let key = format!("session:{session_id}:artifacts");
            let j = serde_json::to_string(a).map_err(|e| format!("json: {e}"))?;
            let _: () = self
                .conn
                .rpush(&key, j)
                .await
                .map_err(|e| format!("rpush: {e}"))?;
            Ok(())
        } else {
            Err("access denied".into())
        }
    }

    pub async fn list_artifacts(&mut self, sid: &Uuid) -> Result<Vec<ArtifactRef>, String> {
        let key = format!("session:{sid}:artifacts");
        let js: Vec<String> = self
            .conn
            .lrange(&key, 0, -1)
            .await
            .map_err(|e| format!("lrange: {e}"))?;
        js.iter()
            .map(|j| serde_json::from_str(j).map_err(|e| format!("parse: {e}")))
            .collect()
    }

    // ── Session Map (Worker context injection) ──

    pub async fn get_session_map(&mut self, sid: &Uuid, wid: &str) -> Result<String, String> {
        let state = self.load(sid).await?.ok_or("session not found")?;
        let short = sid.to_string().chars().take(8).collect::<String>();
        let mut map = format!(
            "SESSION {short}: goal={goal} phase={phase} epoch={epoch}\n",
            goal = state.goal,
            phase = state.current_phase.as_str(),
            epoch = state.plan_epoch
        );

        let pkey = format!("session:{sid}:planner");
        let p: Vec<(String, String)> = self.conn.hgetall(&pkey).await.unwrap_or_default();
        if !p.is_empty() {
            let v = |k: &str| p.iter().find(|(x, _)| x == k).map(|(_, v)| v.clone());
            map.push_str(&format!(
                "  PLANNER: classification={} roles=[{}]\n    Query: get_planner_output()\n",
                v("classification").unwrap_or_default(),
                v("roles").unwrap_or_default()
            ));
        }

        let ws = self.list_workers(sid).await.unwrap_or_default();
        let others: Vec<_> = ws.iter().filter(|w| *w != wid).collect();
        if !others.is_empty() {
            map.push_str(&format!("  WORKERS ({}):", others.len()));
            for w in &others {
                if let Ok(Some(sn)) = self.get_worker_snapshot(sid, w).await {
                    let d = sn.subtasks.iter().filter(|t| t.status == "done").count();
                    map.push_str(&format!(
                        "\n    {w}: {}/{} done → get_worker_output(\"{w}\")",
                        d,
                        sn.subtasks.len()
                    ));
                }
            }
            map.push('\n');
        }

        if let Ok(arts) = self.list_artifacts(sid).await {
            if !arts.is_empty() {
                map.push_str(&format!("  ARTIFACTS ({}):", arts.len()));
                for a in &arts {
                    map.push_str(&format!(
                        "\n    {} ({}) → {}",
                        a.artifact_id, a.artifact_type, a.location
                    ));
                }
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
        let _: () = self
            .conn
            .rpush(&key, entry)
            .await
            .map_err(|e| format!("rpush: {e}"))?;
        Ok(())
    }

    pub async fn get_instructions(&mut self, sid: &Uuid) -> Result<Vec<String>, String> {
        let key = format!("session:{sid}:instructions");
        self.conn
            .lrange(&key, 0, -1)
            .await
            .map_err(|e| format!("lrange: {e}"))
    }

    // ── Internal ──

    async fn write(&mut self, s: &RedisSessionState) -> Result<(), String> {
        let f: Vec<(&str, String)> = vec![
            ("goal", s.goal.clone()),
            ("state", s.state.as_str().into()),
            ("current_phase", s.current_phase.as_str().into()),
            ("plan_version", s.plan_version.to_string()),
            ("plan_epoch", s.plan_epoch.to_string()),
            ("workspace", s.workspace.clone()),
            (
                "pending_gate_id",
                s.pending_gate_id.map(|i| i.to_string()).unwrap_or_default(),
            ),
            ("created_at", s.created_at.to_rfc3339()),
            ("updated_at", chrono::Utc::now().to_rfc3339()),
        ];
        let _: () = self
            .conn
            .hset_multiple(Self::sk(&s.session_id), &f)
            .await
            .map_err(|e| format!("hset: {e}"))?;
        Ok(())
    }

    async fn ttl(&mut self, id: &Uuid) -> Result<(), String> {
        let _: bool = self
            .conn
            .expire(Self::sk(id), self.ttl_seconds as i64)
            .await
            .map_err(|e| format!("{e}"))
            .unwrap_or(false);
        Ok(())
    }

    async fn ttl_key(&mut self, key: &str) -> Result<(), String> {
        let _: bool = self
            .conn
            .expire(key, self.ttl_seconds as i64)
            .await
            .map_err(|e| format!("{e}"))
            .unwrap_or(false);
        Ok(())
    }

    fn to_state(id: &Uuid, f: &[(String, String)]) -> Result<Option<RedisSessionState>, String> {
        let v = |k: &str| f.iter().find(|(x, _)| x == k).map(|(_, v)| v.clone());
        if v("goal").unwrap_or_default().is_empty() {
            return Ok(None);
        }
        Ok(Some(RedisSessionState {
            session_id: *id,
            goal: v("goal").unwrap_or_default(),
            state: v("state")
                .and_then(|x| SessionState::from_str(&x))
                .unwrap_or(SessionState::Active),
            current_phase: v("current_phase")
                .and_then(|x| SessionPhase::from_str(&x))
                .unwrap_or_default(),
            plan_version: v("plan_version").and_then(|x| x.parse().ok()).unwrap_or(0),
            plan_epoch: v("plan_epoch").and_then(|x| x.parse().ok()).unwrap_or(1),
            workspace: v("workspace").unwrap_or_default(),
            pending_gate_id: v("pending_gate_id").and_then(|x| {
                if x.is_empty() {
                    None
                } else {
                    Uuid::parse_str(&x).ok()
                }
            }),
            created_at: v("created_at")
                .and_then(|x| {
                    chrono::DateTime::parse_from_rfc3339(&x)
                        .ok()
                        .map(|t| t.with_timezone(&chrono::Utc))
                })
                .unwrap_or_else(chrono::Utc::now),
            updated_at: v("updated_at")
                .and_then(|x| {
                    chrono::DateTime::parse_from_rfc3339(&x)
                        .ok()
                        .map(|t| t.with_timezone(&chrono::Utc))
                })
                .unwrap_or_else(chrono::Utc::now),
        }))
    }
}

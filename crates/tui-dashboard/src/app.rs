use crossterm::event::{Event, KeyCode, KeyEventKind};
use openforce_proto::swarmos::v1::{
    ApproveApprovalRequestRequest, CancelTaskRequest, CompilePlanRequest, LeaseTaskRequest,
    RejectApprovalRequestRequest,
};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::client::{self, build_command, GrpcClients, SessionSummary};
use crate::components::approval::ApprovalPanel;
use crate::components::command::CommandPanel;
use crate::components::plan_tree::PlanTreePanel;
use crate::components::status::{StatusPanel, TaskInfo};

pub enum ActivePanel {
    Status,
    Command,
    Approval,
    PlanTree,
}

pub struct App {
    pub status: StatusPanel,
    pub command: CommandPanel,
    pub approval: ApprovalPanel,
    pub plan_tree: PlanTreePanel,
    pub active_panel: ActivePanel,
    pub running: bool,
    pub session_id: Uuid,
    pub session_store_addr: String,
    pub scheduler_addr: String,
    pub project_tools_addr: String,
    pub workspace: std::path::PathBuf,
    pub sessions: Vec<SessionSummary>,
    pub clients: Option<GrpcClients>,
    last_refresh: Instant,
    needs_connect: bool,
    pending_action: Option<String>,
}

impl App {
    pub fn new(
        session_store_addr: String,
        scheduler_addr: String,
        project_tools_addr: String,
        workspace: std::path::PathBuf,
    ) -> Self {
        let sessions = client::list_sessions(&workspace);
        let has_sessions = !sessions.is_empty();
        let sid = sessions
            .first()
            .map(|s| s.session_id)
            .unwrap_or(Uuid::nil());

        let mut status = StatusPanel::new();
        if has_sessions {
            status.sessions = sessions.clone();
            status.add_log(&format!(
                "发现 {} 个活跃 Session，输入 session <序号> 切换",
                sessions.len()
            ));
        } else {
            status.add_log("无活跃 Session — 使用 openforce CLI 创建新任务");
        }

        Self {
            status,
            command: CommandPanel::new(),
            approval: ApprovalPanel::new(),
            plan_tree: PlanTreePanel::new(),
            active_panel: ActivePanel::Status,
            running: true,
            session_id: sid,
            session_store_addr,
            scheduler_addr,
            project_tools_addr,
            workspace,
            sessions,
            clients: None,
            last_refresh: Instant::now(),
            needs_connect: !has_sessions,
            pending_action: None,
        }
    }

    pub fn render(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(10), Constraint::Length(10)])
            .split(f.area());

        let main = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(45),
                Constraint::Percentage(30),
                Constraint::Percentage(25),
            ])
            .split(chunks[0]);

        self.status.render(f, main[0]);
        self.plan_tree.render(f, main[1]);
        self.approval.render(f, main[2]);
        self.command.render(f, chunks[1]);

        self.status
            .table_state
            .select(Some(self.status.selected_idx));
    }

    pub fn handle_event(&mut self, ev: Event) {
        if let Event::Key(key) = ev {
            if key.kind == KeyEventKind::Release {
                return;
            }
            match key.code {
                KeyCode::Tab => self.cycle_panel(),
                KeyCode::Up => self.move_selection(-1),
                KeyCode::Down => self.move_selection(1),
                KeyCode::Backspace => self.command.backspace(),
                KeyCode::Enter => {
                    let cmd = self.command.submit();
                    if !cmd.is_empty() {
                        self.execute_command(&cmd);
                    }
                }
                KeyCode::Esc => self.command.clear(),
                KeyCode::Char('q') => self.running = false,
                KeyCode::Char('a') => self.approve_selected(),
                KeyCode::Char('r') => self.reject_selected(),
                KeyCode::Char(c) => self.command.push_char(c),
                _ => {}
            }
        }
    }

    fn cycle_panel(&mut self) {
        self.active_panel = match self.active_panel {
            ActivePanel::Status => ActivePanel::PlanTree,
            ActivePanel::PlanTree => ActivePanel::Command,
            ActivePanel::Command => ActivePanel::Approval,
            ActivePanel::Approval => ActivePanel::Status,
        };
    }

    fn move_selection(&mut self, delta: i32) {
        match self.active_panel {
            ActivePanel::Status => {
                let len = self.status.tasks.len() as i32;
                if len > 0 {
                    self.status.selected_idx =
                        ((self.status.selected_idx as i32 + delta).rem_euclid(len)) as usize;
                }
            }
            ActivePanel::Approval => {
                let len = self.approval.pending.len() as i32;
                if len > 0 {
                    self.approval.selected_idx =
                        ((self.approval.selected_idx as i32 + delta).rem_euclid(len)) as usize;
                }
            }
            ActivePanel::PlanTree => {
                self.plan_tree.move_selection(delta);
            }
            _ => {}
        }
    }

    fn execute_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.splitn(3, ' ').collect();
        match parts.get(0) {
            Some(&"status") | Some(&"s") => {
                self.status.add_log("刷新状态...");
                self.trigger_refresh();
            }
            Some(&"session") | Some(&"sid") => {
                if let Some(id) = parts.get(1) {
                    if let Ok(n) = id.parse::<usize>() {
                        if n >= 1 && n <= self.sessions.len() {
                            let s = &self.sessions[n - 1];
                            self.session_id = s.session_id;
                            self.status
                                .add_log(&format!("切换到 Session {}: {}", n, s.goal));
                            self.needs_connect = true;
                            self.trigger_refresh();
                            return;
                        }
                    }
                    if let Ok(sid) = Uuid::parse_str(id) {
                        self.session_id = sid;
                        self.status.add_log(&format!("切换到 Session: {id}"));
                        self.needs_connect = true;
                        self.trigger_refresh();
                    }
                } else {
                    self.sessions = client::list_sessions(&self.workspace);
                    self.status.sessions = self.sessions.clone();
                    self.status
                        .add_log(&format!("共 {} 个 Session:", self.sessions.len()));
                    for (i, s) in self.sessions.iter().enumerate() {
                        self.status.add_log(&format!(
                            "  {}. [{}] {} — phase={}",
                            i + 1,
                            s.state,
                            s.goal.chars().take(60).collect::<String>(),
                            s.current_phase
                        ));
                    }
                }
            }
            Some(&"lease") => {
                if let Some(tid) = parts.get(1) {
                    self.pending_action = Some(format!("lease:{}", tid));
                    self.status.add_log(&format!("→ 租出任务: {tid}"));
                }
            }
            Some(&"cancel") => {
                if let Some(tid) = parts.get(1) {
                    self.pending_action = Some(format!("cancel:{}", tid));
                    self.status.add_log(&format!("→ 取消任务: {tid}"));
                }
            }
            Some(&"plan") => {
                let desc = parts.get(1).unwrap_or(&"");
                self.pending_action = Some(format!("plan:{}", desc));
                self.status.add_log(&format!("→ 提交计划: {desc}"));
            }
            _ => {
                self.status.add_log(&format!(
                    "未知: {cmd} (可用: status|session|lease|cancel|plan|a批准|r拒绝)"
                ));
            }
        }
    }

    fn approve_selected(&mut self) {
        if let Some(a) = self
            .approval
            .pending
            .get(self.approval.selected_idx)
            .cloned()
        {
            self.pending_action = Some(format!("approve:{}", a.id));
            self.status.add_log(&format!(
                "→ 批准: {} (tool={})",
                &a.id[..12.min(a.id.len())],
                a.tool
            ));
        }
    }

    fn reject_selected(&mut self) {
        if let Some(a) = self
            .approval
            .pending
            .get(self.approval.selected_idx)
            .cloned()
        {
            self.pending_action = Some(format!("reject:{}:manual reject", a.id));
            self.status.add_log(&format!(
                "→ 拒绝: {} (tool={})",
                &a.id[..12.min(a.id.len())],
                a.tool
            ));
        }
    }

    pub fn has_pending_action(&self) -> bool {
        self.pending_action.is_some()
    }

    /// Execute the pending action via gRPC. Called from the async event loop.
    pub async fn execute_pending_action(&mut self) {
        let action = match self.pending_action.take() {
            Some(a) => a,
            None => return,
        };
        if self.session_id.is_nil() {
            self.status.add_log("错误: 未选择 Session");
            return;
        }
        // Ensure connected
        if self.clients.is_none() {
            match GrpcClients::connect(
                &self.session_store_addr,
                &self.scheduler_addr,
                &self.project_tools_addr,
            )
            .await
            {
                Ok(c) => {
                    self.clients = Some(c);
                    self.needs_connect = false;
                }
                Err(e) => {
                    self.status.add_log(&format!("gRPC 连接失败: {e}"));
                    return;
                }
            }
        }
        let clients = self.clients.as_mut().unwrap();
        let mut parts = action.splitn(3, ':');
        let cmd_type = parts.next().unwrap_or("");

        match cmd_type {
            "lease" => {
                let task_id = parts.next().unwrap_or("");
                let req = LeaseTaskRequest {
                    command: Some(build_command(
                        "LeaseTask",
                        &self.session_id,
                        Some(task_id),
                        vec![],
                    )),
                };
                match clients.scheduler.lease_task(req).await {
                    Ok(resp) => {
                        let r = resp.into_inner();
                        self.status.add_log(&format!(
                            "✓ 租出成功: lease={:.12} fence={}",
                            r.lease_id, r.fencing_token
                        ));
                        self.trigger_refresh();
                    }
                    Err(e) => self.status.add_log(&format!("✗ 租出失败: {e}")),
                }
            }
            "cancel" => {
                let task_id = parts.next().unwrap_or("");
                let req = CancelTaskRequest {
                    command: Some(build_command(
                        "CancelTask",
                        &self.session_id,
                        Some(task_id),
                        vec![],
                    )),
                    task_id: task_id.to_string(),
                    reason: "TUI manual cancel".to_string(),
                };
                match clients.scheduler.cancel_task(req).await {
                    Ok(_) => {
                        self.status.add_log(&format!("✓ 已取消: {task_id}"));
                        self.trigger_refresh();
                    }
                    Err(e) => self.status.add_log(&format!("✗ 取消失败: {e}")),
                }
            }
            "plan" => {
                let desc = parts.next().unwrap_or("");
                let req = CompilePlanRequest {
                    command: Some(build_command(
                        "CompilePlan",
                        &self.session_id,
                        None,
                        desc.as_bytes().to_vec(),
                    )),
                };
                match clients.scheduler.compile_plan(req).await {
                    Ok(resp) => {
                        let r = resp.into_inner();
                        self.status.add_log(&format!(
                            "✓ 计划提交: plan_v{} epoch#{}",
                            r.plan_version, r.plan_epoch
                        ));
                        self.trigger_refresh();
                    }
                    Err(e) => self.status.add_log(&format!("✗ 计划失败: {e}")),
                }
            }
            "approve" => {
                let approval_id = parts.next().unwrap_or("");
                let req = ApproveApprovalRequestRequest {
                    approval_request_id: approval_id.to_string(),
                    approver_id: "tui-operator".to_string(),
                    approved_at: Some(prost_types::Timestamp {
                        seconds: chrono::Utc::now().timestamp(),
                        nanos: 0,
                    }),
                    usage_limit: 1,
                };
                match clients.approval.approve_approval_request(req).await {
                    Ok(_) => {
                        self.status
                            .add_log(&format!("✓ 已批准: {:.12}", approval_id));
                        self.approval.pending.retain(|a| a.id != approval_id);
                        self.trigger_refresh();
                    }
                    Err(e) => self.status.add_log(&format!("✗ 批准失败: {e}")),
                }
            }
            "reject" => {
                let approval_id = parts.next().unwrap_or("");
                let reason = parts.next().unwrap_or("manual reject");
                let req = RejectApprovalRequestRequest {
                    approval_request_id: approval_id.to_string(),
                    approver_id: "tui-operator".to_string(),
                    reason: reason.to_string(),
                    rejected_at: Some(prost_types::Timestamp {
                        seconds: chrono::Utc::now().timestamp(),
                        nanos: 0,
                    }),
                };
                match clients.approval.reject_approval_request(req).await {
                    Ok(_) => {
                        self.status
                            .add_log(&format!("✓ 已拒绝: {:.12}", approval_id));
                        self.approval.pending.retain(|a| a.id != approval_id);
                        self.trigger_refresh();
                    }
                    Err(e) => self.status.add_log(&format!("✗ 拒绝失败: {e}")),
                }
            }
            _ => self.status.add_log(&format!("未知操作: {cmd_type}")),
        }
    }

    pub fn trigger_refresh(&mut self) {
        self.last_refresh = Instant::now() - Duration::from_secs(10);
    }

    pub fn needs_refresh(&self) -> bool {
        self.last_refresh.elapsed() > Duration::from_secs(3)
    }

    pub fn mark_refreshed(&mut self) {
        self.last_refresh = Instant::now();
    }

    pub async fn refresh_data(&mut self) {
        if self.session_id.is_nil() {
            self.mark_refreshed();
            return;
        }

        if self.clients.is_none() || self.needs_connect {
            match GrpcClients::connect(
                &self.session_store_addr,
                &self.scheduler_addr,
                &self.project_tools_addr,
            )
            .await
            {
                Ok(c) => {
                    self.clients = Some(c);
                    self.needs_connect = false;
                    self.status.add_log("gRPC 已连接");
                }
                Err(e) => {
                    self.mark_refreshed();
                    self.status.add_log(&format!("gRPC 连接失败: {e}"));
                    return;
                }
            }
        }

        let clients = self.clients.as_mut().unwrap();

        match client::fetch_session_info(&mut clients.session_store, &self.session_id).await {
            Ok(info) => {
                self.status.session_goal = info["goal"].as_str().unwrap_or("").to_string();
                self.status.session_state = info["state"].as_str().unwrap_or("?").to_string();
                self.status.plan_version = info["plan_version"].as_i64().unwrap_or(0) as i32;
                self.plan_tree.plan_epoch = info["plan_epoch"].as_i64().unwrap_or(0) as i32;
            }
            Err(e) => self.status.add_log(&format!("获取 Session 失败: {e}")),
        }

        match client::fetch_tasks(&mut clients.session_store, &self.session_id).await {
            Ok(tasks) => {
                self.status.tasks = tasks
                    .iter()
                    .map(|t| TaskInfo {
                        task_id: t["task_id"].as_str().unwrap_or("?").to_string(),
                        task_type: t["task_type"].as_str().unwrap_or("?").to_string(),
                        state: t["state"].as_str().unwrap_or("?").to_string(),
                        attempt: t["attempt"].as_i64().unwrap_or(0) as i32,
                        fencing: t["fencing"].as_u64().unwrap_or(0),
                        lease_id: t["lease_id"].as_str().unwrap_or("-").to_string(),
                    })
                    .collect();
                self.plan_tree.update_from_tasks(&self.status.tasks);
            }
            Err(e) => self.status.add_log(&format!("获取任务失败: {e}")),
        }

        self.sessions = client::list_sessions(&self.workspace);
        self.status.sessions = self.sessions.clone();
        self.mark_refreshed();
    }
}

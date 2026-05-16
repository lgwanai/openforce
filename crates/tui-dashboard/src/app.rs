use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};
use std::time::{Duration, Instant};
use tonic::transport::Channel;
use uuid::Uuid;

use openforce_proto::swarmos::v1::session_store_client::SessionStoreClient;
use crate::client::{fetch_session_info, fetch_tasks};
use crate::components::status::{StatusPanel, TaskInfo};
use crate::components::command::CommandPanel;
use crate::components::approval::{ApprovalPanel, PendingApproval};

pub enum ActivePanel { Status, Command, Approval }

pub struct App {
    pub status: StatusPanel,
    pub command: CommandPanel,
    pub approval: ApprovalPanel,
    pub active_panel: ActivePanel,
    pub running: bool,
    pub session_id: Uuid,
    pub session_store_addr: String,
    pub scheduler_addr: String,
    pub project_tools_addr: String,
    last_refresh: Instant,
}

impl App {
    pub fn new(
        session_store_addr: String,
        scheduler_addr: String,
        project_tools_addr: String,
    ) -> Self {
        Self {
            status: StatusPanel::new(),
            command: CommandPanel::new(),
            approval: ApprovalPanel::new(),
            active_panel: ActivePanel::Status,
            running: true,
            session_id: Uuid::nil(),
            session_store_addr,
            scheduler_addr,
            project_tools_addr,
            last_refresh: Instant::now(),
        }
    }

    pub fn render(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(10),
                Constraint::Length(10),
            ])
            .split(f.area());

        // Main content: status + approvals side by side
        let main = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(60),
                Constraint::Percentage(40),
            ])
            .split(chunks[0]);

        self.status.render(f, main[0]);
        self.approval.render(f, main[1]);

        // Bottom: command panel
        self.command.render(f, chunks[1]);

        self.status.table_state.select(Some(self.status.selected_idx));
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
            ActivePanel::Status => ActivePanel::Command,
            ActivePanel::Command => ActivePanel::Approval,
            ActivePanel::Approval => ActivePanel::Status,
        };
    }

    fn move_selection(&mut self, delta: i32) {
        match self.active_panel {
            ActivePanel::Status => {
                let len = self.status.tasks.len() as i32;
                if len > 0 {
                    self.status.selected_idx = ((self.status.selected_idx as i32 + delta).rem_euclid(len)) as usize;
                }
            }
            ActivePanel::Approval => {
                let len = self.approval.pending.len() as i32;
                if len > 0 {
                    self.approval.selected_idx = ((self.approval.selected_idx as i32 + delta).rem_euclid(len)) as usize;
                }
            }
            _ => {}
        }
    }

    fn execute_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.splitn(3, ' ').collect();
        match parts.get(0) {
            Some(&"status") => {
                self.status.add_log("刷新状态中...");
                self.trigger_refresh();
            }
            Some(&"session") => {
                if let Some(id) = parts.get(1) {
                    if let Ok(sid) = Uuid::parse_str(id) {
                        self.session_id = sid;
                        self.status.add_log(&format!("切换到 Session: {id}"));
                        self.trigger_refresh();
                    }
                }
            }
            Some(&"lease") => {
                self.status.add_log(&format!("租出任务: {}", parts.get(1).unwrap_or(&"?")));
            }
            Some(&"cancel") => {
                self.status.add_log(&format!("取消任务: {}", parts.get(1).unwrap_or(&"?")));
            }
            Some(&"plan") => {
                let desc = parts.get(1).unwrap_or(&"新计划");
                self.status.add_log(&format!("提交计划: {desc}"));
            }
            _ => {
                self.status.add_log(&format!("未知指令: {cmd}"));
            }
        }
    }

    fn approve_selected(&mut self) {
        if let Some(a) = self.approval.pending.get(self.approval.selected_idx).cloned() {
            self.status.add_log(&format!("批准: {} (tool={})", a.id, a.tool));
            self.approval.pending.remove(self.approval.selected_idx);
        }
    }

    fn reject_selected(&mut self) {
        if let Some(a) = self.approval.pending.get(self.approval.selected_idx).cloned() {
            self.status.add_log(&format!("拒绝: {} (tool={})", a.id, a.tool));
            self.approval.pending.remove(self.approval.selected_idx);
        }
    }

    fn trigger_refresh(&mut self) {
        self.last_refresh = Instant::now() - Duration::from_secs(10); // force refresh
    }

    pub fn needs_refresh(&self) -> bool {
        self.last_refresh.elapsed() > Duration::from_secs(3)
    }

    pub fn mark_refreshed(&mut self) {
        self.last_refresh = Instant::now();
    }

    pub async fn refresh_data(&mut self) {
        // In production: connect to gRPC and fetch real data
        // For now: populate with demo data if empty
        if self.status.tasks.len() == 0 && !self.session_id.is_nil() {
            return; // will populate when connected
        }
        self.mark_refreshed();
    }
}

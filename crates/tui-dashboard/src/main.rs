use std::io;
use std::time::{Duration, Instant};
use crossterm::{
    event::{self, Event, EnableMouseCapture, DisableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::{Frame, Terminal};

mod client;
mod components;
mod app;

use app::App;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let session_store_addr = std::env::var("SESSION_STORE_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:50051".into());
    let scheduler_addr = std::env::var("SCHEDULER_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:50052".into());
    let project_tools_addr = std::env::var("PROJECT_TOOLS_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:50053".into());

    let mut app = App::new(
        session_store_addr, scheduler_addr, project_tools_addr,
    );

    app.status.add_log("SwarmOS TUI v5.1 — 输入 'status' 刷新 | 'q' 退出 | Tab 切换面板");

    // Demo data for offline display
    app.status.tasks = vec![
        components::status::TaskInfo {
            task_id: "task_1a2b3c4d".into(),
            task_type: "backend_crud".into(),
            state: "Running".into(),
            attempt: 1,
            fencing: 5,
            lease_id: "lease_x1".into(),
        },
        components::status::TaskInfo {
            task_id: "task_2b3c4d5e".into(),
            task_type: "frontend_page".into(),
            state: "Ready".into(),
            attempt: 0,
            fencing: 0,
            lease_id: "-".into(),
        },
        components::status::TaskInfo {
            task_id: "task_3c4d5e6f".into(),
            task_type: "integration_test".into(),
            state: "Succeeded".into(),
            attempt: 2,
            fencing: 8,
            lease_id: "lease_z3".into(),
        },
        components::status::TaskInfo {
            task_id: "task_4d5e6f7g".into(),
            task_type: "security_review".into(),
            state: "Failed".into(),
            attempt: 3,
            fencing: 12,
            lease_id: "lease_w9".into(),
        },
    ];

    app.approval.pending = vec![
        components::approval::PendingApproval {
            id: "approv_x1".into(),
            tool: "delete_project_file".into(),
            risk: "sensitive".into(),
            paths: vec!["backend/auth/old_service.go".into()],
            created: "14:30".into(),
        },
        components::approval::PendingApproval {
            id: "approv_y2".into(),
            tool: "write_project_patch".into(),
            risk: "moderate".into(),
            paths: vec!["infra/config.yaml".into()],
            created: "14:32".into(),
        },
    ];

    app.status.session_goal = "构建图书管理系统".into();
    app.status.session_state = "active".into();
    app.status.plan_version = 7;

    // Main event loop
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| app.render(f))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            let ev = event::read()?;
            app.handle_event(ev);
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }

        if !app.running {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

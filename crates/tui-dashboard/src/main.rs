use std::io;
use std::time::{Duration, Instant};
use crossterm::{
    event::{self, EnableMouseCapture, DisableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

mod client;
mod components;
mod app;

use app::App;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
    let workspace = std::env::current_dir().unwrap_or_default();

    let mut app = App::new(
        session_store_addr, scheduler_addr, project_tools_addr, workspace,
    );

    let tick_rate = Duration::from_millis(250);
    let refresh_interval = Duration::from_secs(3);
    let mut last_tick = Instant::now();
    let mut last_refresh = Instant::now();

    loop {
        terminal.draw(|f| app.render(f))?;

        // Execute any pending gRPC action from user input
        if app.has_pending_action() {
            app.execute_pending_action().await;
        }

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::ZERO);

        if event::poll(timeout)? {
            let ev = event::read()?;
            app.handle_event(ev);
        }

        if last_tick.elapsed() >= tick_rate {
            if last_refresh.elapsed() >= refresh_interval || app.needs_refresh() {
                app.refresh_data().await;
                last_refresh = Instant::now();
            }
            last_tick = Instant::now();
        }

        if !app.running {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

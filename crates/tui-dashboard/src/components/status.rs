use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Row, Table, TableState, Cell},
    Frame,
};

#[derive(Clone)]
pub struct TaskInfo {
    pub task_id: String,
    pub task_type: String,
    pub state: String,
    pub attempt: i32,
    pub fencing: u64,
    pub lease_id: String,
}

pub struct StatusPanel {
    pub tasks: Vec<TaskInfo>,
    pub session_goal: String,
    pub session_state: String,
    pub plan_version: i32,
    pub table_state: TableState,
    pub selected_idx: usize,
    pub log_messages: Vec<String>,
}

impl StatusPanel {
    pub fn new() -> Self {
        Self {
            tasks: vec![],
            session_goal: String::new(),
            session_state: "active".into(),
            plan_version: 0,
            table_state: TableState::default(),
            selected_idx: 0,
            log_messages: vec![],
        }
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(12),
                Constraint::Min(3),
            ])
            .split(area);

        // Session header
        let header = Paragraph::new(Line::from(vec![
            Span::styled(format!("  Session: {}  ", &self.session_state), Style::default().fg(Color::Green)),
            Span::styled(format!("Plan v{}  ", self.plan_version), Style::default().fg(Color::Cyan)),
            Span::raw(&self.session_goal),
        ]))
        .block(Block::default().borders(Borders::TOP).title("Session"));
        f.render_widget(header, chunks[0]);

        // Task table
        let header_cells = ["Task ID", "Type", "State", "Attempt", "Fencing"];
        let header = Row::new(header_cells.iter().map(|h| Cell::from(*h)))
            .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

        let rows: Vec<Row> = self.tasks.iter().enumerate().map(|(i, t)| {
            let color = match t.state.as_str() {
                "Running" => Color::Green,
                "Succeeded" => Color::Blue,
                "Failed" => Color::Red,
                "TimedOut" => Color::Magenta,
                "Leased" => Color::Cyan,
                "Ready" => Color::Yellow,
                _ => Color::Gray,
            };
            let style = if i == self.selected_idx {
                Style::default().fg(Color::Black).bg(color)
            } else {
                Style::default().fg(color)
            };
            Row::new(vec![
                Cell::from(t.task_id.clone()),
                Cell::from(t.task_type.clone()),
                Cell::from(t.state.clone()),
                Cell::from(t.attempt.to_string()),
                Cell::from(t.fencing.to_string()),
            ]).style(style)
        }).collect();

        let table = Table::new(rows, [
            Constraint::Length(12),
            Constraint::Length(14),
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Length(8),
        ])
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("Agent Tasks"))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        f.render_stateful_widget(table, chunks[1], &mut self.table_state);

        // Log messages
        let log_text: Vec<Line> = self.log_messages.iter().rev().take(10)
            .map(|m| Line::from(Span::raw(m))).collect();
        let log = Paragraph::new(Text::from(log_text))
            .block(Block::default().borders(Borders::ALL).title("Log"));
        f.render_widget(log, chunks[2]);
    }

    pub fn add_log(&mut self, msg: &str) {
        self.log_messages.push(format!("[{}] {}", chrono::Local::now().format("%H:%M:%S"), msg));
    }
}

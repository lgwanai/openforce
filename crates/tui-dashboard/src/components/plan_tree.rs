use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};
use crate::components::status::TaskInfo;

pub struct PlanTreePanel {
    pub tasks: Vec<TaskInfo>,
    pub plan_epoch: i32,
    pub selected_idx: usize,
}

impl PlanTreePanel {
    pub fn new() -> Self {
        Self { tasks: vec![], plan_epoch: 0, selected_idx: 0 }
    }

    pub fn update_from_tasks(&mut self, tasks: &[TaskInfo]) {
        self.tasks = tasks.to_vec();
    }

    pub fn move_selection(&mut self, delta: i32) {
        let len = self.tasks.len() as i32;
        if len > 0 {
            self.selected_idx = ((self.selected_idx as i32 + delta).rem_euclid(len)) as usize;
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .split(area);

        let header = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("  Epoch {}  ", self.plan_epoch),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("{} tasks", self.tasks.len())),
        ]))
        .block(Block::default().borders(Borders::TOP));
        f.render_widget(header, chunks[0]);

        if self.tasks.is_empty() {
            let empty = Paragraph::new("No tasks")
                .block(Block::default().borders(Borders::ALL).title("Plan Tree"));
            f.render_widget(empty, chunks[1]);
            return;
        }

        // Group tasks by type for tree-like display
        let mut items: Vec<ListItem> = vec![];
        for (i, task) in self.tasks.iter().enumerate() {
            let state_color = match task.state.as_str() {
                "Running" | "Leased" => Color::Green,
                "Succeeded" => Color::Blue,
                "Failed" | "TimedOut" => Color::Red,
                "Ready" => Color::Yellow,
                "Pending" => Color::Gray,
                "Cancelled" => Color::Magenta,
                _ => Color::White,
            };
            let prefix = if i == self.selected_idx { "▶ " } else { "  " };
            let style = if i == self.selected_idx {
                Style::default().fg(Color::Black).bg(state_color)
            } else {
                Style::default().fg(state_color)
            };

            let mut spans = vec![
                Span::styled(prefix, style),
                Span::styled(format!("[{}] ", task.task_id.chars().take(10).collect::<String>()), style),
                Span::styled(&task.task_type, style.add_modifier(Modifier::BOLD)),
                Span::styled(format!(" ({})", task.state), style),
            ];
            if task.attempt > 0 {
                spans.push(Span::styled(format!(" attempt#{}", task.attempt), Style::default().fg(Color::DarkGray)));
            }
            if task.fencing > 0 {
                spans.push(Span::styled(format!(" fence#{}", task.fencing), Style::default().fg(Color::DarkGray)));
            }
            items.push(ListItem::new(Line::from(spans)));
        }

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Task Decomposition"))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        f.render_widget(list, chunks[1]);
    }
}

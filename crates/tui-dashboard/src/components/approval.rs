use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, List, ListItem},
    Frame,
};

#[derive(Clone)]
pub struct PendingApproval {
    pub id: String,
    pub tool: String,
    pub risk: String,
    pub paths: Vec<String>,
    pub created: String,
}

pub struct ApprovalPanel {
    pub pending: Vec<PendingApproval>,
    pub selected_idx: usize,
}

impl ApprovalPanel {
    pub fn new() -> Self { Self { pending: vec![], selected_idx: 0 } }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .split(area);

        let status = format!("  待审批: {} 项", self.pending.len());
        let header = Paragraph::new(Span::styled(status, Style::default().fg(Color::Yellow)))
            .block(Block::default().borders(Borders::TOP));
        f.render_widget(header, chunks[0]);

        let items: Vec<ListItem> = self.pending.iter().enumerate().map(|(i, a)| {
            let style = if i == self.selected_idx {
                Style::default().fg(Color::Black).bg(Color::Yellow)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("[{}] ", a.id.chars().take(8).collect::<String>()), style),
                Span::styled(&a.tool, style.add_modifier(Modifier::BOLD)),
                Span::styled(format!(" risk={} paths={:?}", a.risk, a.paths), style),
            ]))
        }).collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Approvals [A批准 R拒绝]"));
        f.render_widget(list, chunks[1]);
    }
}

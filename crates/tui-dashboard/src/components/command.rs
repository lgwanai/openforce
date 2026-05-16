use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, List, ListItem},
    Frame,
};

pub struct CommandPanel {
    pub input: String,
    pub cursor_pos: usize,
    pub history: Vec<String>,
    pub suggestions: Vec<String>,
    pub mode: String,
}

impl CommandPanel {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            cursor_pos: 0,
            history: Vec::new(),
            suggestions: vec![
                "plan: <description>  - 提交新的编排计划".into(),
                "lease <task_id>     - 租出一个Ready任务".into(),
                "cancel <task_id>    - 取消一个运行中的任务".into(),
                "approve <req_id>    - 批准审批请求".into(),
                "reject <req_id>     - 拒绝审批请求".into(),
                "status              - 刷新所有状态".into(),
                "session <id>        - 切换到指定Session".into(),
            ],
            mode: "COMMAND".into(),
        }
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
            ])
            .split(area);

        // Input area
        let input_text = format!("> {}", self.input);
        let cursor = if self.input.len() < self.cursor_pos {
            self.input.len()
        } else {
            self.cursor_pos
        };
        let display = if self.input.is_empty() {
            Line::from(vec![
                Span::styled("> ", Style::default().fg(Color::Green)),
                Span::styled("输入指令 (Tab 切换面板)...", Style::default().fg(Color::DarkGray)),
            ])
        } else {
            Line::from(vec![
                Span::styled("> ", Style::default().fg(Color::Green)),
                Span::raw(&self.input),
            ])
        };

        let input_widget = Paragraph::new(display)
            .block(Block::default().borders(Borders::ALL)
                .title(format!(" {} Mode [Enter 执行] [Esc 清空] ", self.mode)))
            .style(Style::default().fg(Color::White));
        f.render_widget(input_widget, chunks[0]);

        // Suggestions / History
        let items: Vec<ListItem> = self.suggestions.iter()
            .map(|s| ListItem::new(Span::styled(s, Style::default().fg(Color::DarkGray))))
            .collect();
        let suggestions = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Commands"));
        f.render_widget(suggestions, chunks[1]);
    }

    pub fn push_char(&mut self, c: char) {
        self.input.push(c);
        self.cursor_pos = self.input.len();
    }

    pub fn backspace(&mut self) {
        if self.cursor_pos > 0 {
            self.input.remove(self.cursor_pos - 1);
            self.cursor_pos -= 1;
        }
    }

    pub fn clear(&mut self) {
        self.input.clear();
        self.cursor_pos = 0;
    }

    pub fn submit(&mut self) -> String {
        let cmd = self.input.clone();
        if !cmd.is_empty() {
            self.history.push(cmd.clone());
        }
        self.clear();
        cmd
    }
}

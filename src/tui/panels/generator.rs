use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub struct GeneratorState {
    pub collapsed: bool,
}

impl GeneratorState {
    pub fn new() -> Self {
        Self { collapsed: false }
    }

    pub fn toggle_collapsed(&mut self) {
        self.collapsed = !self.collapsed;
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, is_active: bool) {
        let border_style = if is_active {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title(" DTS Generator (g to toggle) ")
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let msg = Paragraph::new("Coming soon\n\nThis panel will allow you to:\n• Pick a board\n• Choose a save location\n• Reference & update existing nodes\n• Create new nodes with properties")
            .style(Style::default().fg(Color::DarkGray))
            .wrap(Wrap { trim: true });
        frame.render_widget(msg, inner);
    }
}

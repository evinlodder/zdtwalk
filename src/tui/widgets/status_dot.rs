use ratatui::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusColor {
    Okay,
    Disabled,
    Unknown,
    /// No status property exists on this node — show no dot.
    None,
}

impl StatusColor {
    pub fn dot_span(self) -> Span<'static> {
        match self {
            StatusColor::Okay => Span::styled("●", Style::default().fg(Color::Green)),
            StatusColor::Disabled => Span::styled("●", Style::default().fg(Color::Red)),
            StatusColor::Unknown => Span::styled("●", Style::default().fg(Color::DarkGray)),
            StatusColor::None => Span::raw(" "),
        }
    }
}

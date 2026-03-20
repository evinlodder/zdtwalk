use ratatui::prelude::*;

use crate::tui::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusColor {
    Okay,
    Disabled,
    Unknown,
    /// No status property exists on this node — show no dot.
    None,
}

impl StatusColor {
    #[allow(dead_code)]
    pub fn dot_span(self) -> Span<'static> {
        match self {
            StatusColor::Okay => Span::styled("●", Style::default().fg(theme::SUCCESS)),
            StatusColor::Disabled => Span::styled("●", Style::default().fg(theme::ERROR)),
            StatusColor::Unknown => Span::styled("●", Style::default().fg(theme::TEXT_DIM)),
            StatusColor::None => Span::raw(" "),
        }
    }
}
